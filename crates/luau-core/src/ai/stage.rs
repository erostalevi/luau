//! Stage inference from lane names (SPEC §12.3). There are no fixed "done"
//! lanes: a multilingual (en/es/pt) keyword dictionary classifies each lane.
//! Matching is case/accent-insensitive, on whole words, and ignores emoji and
//! numbering ("✅ Done", "3. Hecho", "DONE!" all match).

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Todo,
    InProgress,
    Done,
    #[default]
    Other,
}

const DONE: &[&str] = &[
    // en
    "done",
    "complete",
    "completed",
    "finished",
    "finish",
    "closed",
    "resolved",
    "shipped",
    "released",
    "deployed",
    "archived",
    "delivered",
    "merged",
    "accepted",
    "live",
    // es
    "hecho",
    "hechos",
    "hecha",
    "hechas",
    "terminado",
    "terminados",
    "terminada",
    "terminadas",
    "listo",
    "listos",
    "lista",
    "listas",
    "completado",
    "completados",
    "completada",
    "completadas",
    "cerrado",
    "cerrados",
    "cerrada",
    "cerradas",
    "finalizado",
    "finalizados",
    "finalizada",
    "finalizadas",
    "resuelto",
    "resueltos",
    "resuelta",
    "resueltas",
    "entregado",
    "entregados",
    "entregada",
    "entregadas",
    "publicado",
    "desplegado",
    // pt
    "concluido",
    "concluidos",
    "concluida",
    "concluidas",
    "feito",
    "feitos",
    "feita",
    "feitas",
    "pronto",
    "prontos",
    "pronta",
    "prontas",
    "fechado",
    "fechados",
    "fechada",
    "fechadas",
    "entregue",
    "entregues",
    "resolvido",
    "resolvidos",
    "completo",
    "completos",
    "finalizado",
];

const DOING: &[&str] = &[
    // en
    "doing",
    "in progress",
    "progress",
    "wip",
    "working",
    "active",
    "started",
    "ongoing",
    "review",
    "in review",
    "code review",
    "reviewing",
    "testing",
    "test",
    "qa",
    "validation",
    "blocked",
    "development",
    "dev",
    "implementing",
    "now",
    "today",
    "current",
    // es
    "en curso",
    "en progreso",
    "haciendo",
    "en proceso",
    "revision",
    "en revision",
    "trabajando",
    "pruebas",
    "desarrollo",
    "hoy",
    "bloqueado",
    "activo",
    "en marcha",
    // pt
    "fazendo",
    "em andamento",
    "andamento",
    "em progresso",
    "em curso",
    "revisao",
    "em revisao",
    "testando",
    "desenvolvimento",
    "hoje",
    "bloqueada",
    "ativo",
];

const TODO: &[&str] = &[
    // en
    "todo",
    "to do",
    "backlog",
    "next",
    "up next",
    "planned",
    "planning",
    "inbox",
    "ideas",
    "idea",
    "later",
    "someday",
    "queue",
    "queued",
    "ready",
    "new",
    "open",
    "pending",
    "icebox",
    "wishlist",
    // es
    "pendiente",
    "pendientes",
    "por hacer",
    "a hacer",
    "hacer",
    "siguiente",
    "siguientes",
    "proximo",
    "proximos",
    "planificado",
    "bandeja de entrada",
    "entrada",
    "tareas",
    "ideas",
    "algun dia",
    "despues",
    // pt
    "a fazer",
    "fazer",
    "pendente",
    "pendentes",
    "ideias",
    "planejado",
    "caixa de entrada",
    "proximas",
    "depois",
    "fila",
];

/// "ready for review", "listo para …", "pronto para …" mean *not yet done*.
const READY_FOR: &[&str] = &[
    "ready for",
    "ready to",
    "listo para",
    "lista para",
    "listos para",
    "pronto para",
    "pronta para",
];

/// Lowercase, strip accents, replace everything that is not a letter or digit
/// by a space, drop pure-number tokens (numbering), collapse spaces.
pub fn normalize(s: &str) -> String {
    let folded: String = s
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    folded
        .split_whitespace()
        .filter(|t| !t.chars().all(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_phrase(padded: &str, phrase: &str) -> bool {
    padded.contains(&format!(" {phrase} "))
}

/// Classify a lane by its name.
pub fn infer_stage(lane_name: &str) -> Stage {
    let n = normalize(lane_name);
    if n.is_empty() {
        return Stage::Other;
    }
    // "ToDo" → "todo", "To-Do" → "to do": both are listed.
    let padded = format!(" {n} ");
    if READY_FOR.iter().any(|p| has_phrase(&padded, p)) {
        // Could still be "ready for review" (a review queue ≈ in progress).
        if has_phrase(&padded, "review")
            || has_phrase(&padded, "revision")
            || has_phrase(&padded, "revisao")
        {
            return Stage::InProgress;
        }
        return Stage::Todo;
    }
    if has_phrase(&padded, "not done")
        || has_phrase(&padded, "sin hacer")
        || has_phrase(&padded, "no hecho")
    {
        return Stage::Todo;
    }
    if DONE.iter().any(|p| has_phrase(&padded, p)) {
        return Stage::Done;
    }
    if DOING.iter().any(|p| has_phrase(&padded, p)) {
        return Stage::InProgress;
    }
    if TODO.iter().any(|p| has_phrase(&padded, p)) {
        return Stage::Todo;
    }
    Stage::Other
}

/// Map a Jira-style status category (`todo`/`new`, `inProgress`/`indeterminate`, `done`).
pub fn from_status_category(cat: &str) -> Option<Stage> {
    match normalize(cat).replace(' ', "").as_str() {
        "done" | "complete" => Some(Stage::Done),
        "inprogress" | "indeterminate" => Some(Stage::InProgress),
        "todo" | "new" | "undefined" => Some(Stage::Todo),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_emoji_numbers_accents() {
        assert_eq!(normalize("✅ 3. Concluído!"), "concluido");
        assert_eq!(normalize("  EN   Revisión "), "en revision");
        assert_eq!(normalize("To-Do"), "to do");
    }

    #[test]
    fn done_lanes() {
        for n in [
            "Done",
            "✅ Done",
            "3. Hecho",
            "DONE!",
            "Terminado",
            "Listo 🎉",
            "Concluído",
            "Cerradas",
            "Shipped",
            "Feito",
        ] {
            assert_eq!(infer_stage(n), Stage::Done, "{n}");
        }
    }

    #[test]
    fn doing_lanes() {
        for n in [
            "Doing",
            "In progress",
            "In-Progress",
            "WIP",
            "En curso",
            "Em andamento",
            "Fazendo",
            "Code review",
            "QA",
        ] {
            assert_eq!(infer_stage(n), Stage::InProgress, "{n}");
        }
    }

    #[test]
    fn todo_lanes() {
        for n in [
            "To do",
            "TODO",
            "ToDo",
            "Backlog",
            "📥 Inbox",
            "Pendiente",
            "Por hacer",
            "A fazer",
            "Up next",
            "Ready for dev",
        ] {
            assert_eq!(infer_stage(n), Stage::Todo, "{n}");
        }
        assert_eq!(infer_stage("Ready for review"), Stage::InProgress);
    }

    #[test]
    fn other_lanes_and_whole_words() {
        assert_eq!(infer_stage("Notes"), Stage::Other);
        assert_eq!(infer_stage("Reference"), Stage::Other);
        // Whole words only: "undone" and "abandoned" must not match "done".
        assert_eq!(infer_stage("Undone"), Stage::Other);
        assert_eq!(infer_stage("Abandoned"), Stage::Other);
        assert_eq!(infer_stage(""), Stage::Other);
        assert_eq!(infer_stage("🔥"), Stage::Other);
    }

    #[test]
    fn status_category() {
        assert_eq!(from_status_category("done"), Some(Stage::Done));
        assert_eq!(from_status_category("inProgress"), Some(Stage::InProgress));
        assert_eq!(
            from_status_category("indeterminate"),
            Some(Stage::InProgress)
        );
        assert_eq!(from_status_category("To Do"), Some(Stage::Todo));
        assert_eq!(from_status_category("weird"), None);
    }
}
