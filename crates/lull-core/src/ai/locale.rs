//! Localized strings for the deterministic summary (en / es / pt).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    En,
    Es,
    Pt,
}

impl Locale {
    pub fn parse(s: &str) -> Locale {
        match s.trim().to_lowercase().get(..2) {
            Some("es") => Locale::Es,
            Some("pt") => Locale::Pt,
            _ => Locale::En,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Es => "es",
            Locale::Pt => "pt",
        }
    }
    /// Language name used in AI prompts.
    pub fn language(self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Es => "Spanish (español)",
            Locale::Pt => "Portuguese (português)",
        }
    }
    pub fn strings(self) -> &'static Strings {
        match self {
            Locale::En => &EN,
            Locale::Es => &ES,
            Locale::Pt => &PT,
        }
    }
}

pub struct Strings {
    pub title: &'static str,
    pub no_activity: &'static str,
    pub completed: &'static str,
    pub started: &'static str,
    pub created: &'static str,
    pub moved: &'static str,
    pub edited: &'static str,
    pub deleted: &'static str,
    pub archived: &'static str,
    pub external: &'static str,
    pub pending: &'static str,
    pub overdue: &'static str,
    pub priorities: &'static str,
    pub weekend: &'static str,
    /// `{n}` more items.
    pub more: &'static str,
    /// `{n}` cards edited.
    pub edited_count: &'static str,
    /// `{n}` sessions.
    pub sessions: &'static str,
    /// `{n}` chars.
    pub chars: &'static str,
    pub due: &'static str,
    pub priority: &'static str,
    pub another_board: &'static str,
    pub open_count: &'static str,
    pub weekdays: [&'static str; 7],
    pub months: [&'static str; 12],
}

pub static EN: Strings = Strings {
    title: "Activity summary",
    no_activity: "No activity in this period.",
    completed: "Completed",
    started: "Started",
    created: "Created",
    moved: "Moved",
    edited: "Edited",
    deleted: "Deleted",
    archived: "Archived",
    external: "Changed outside the app",
    pending: "Pending",
    overdue: "Overdue",
    priorities: "Priorities",
    weekend: "Over the weekend",
    more: "+{n} more",
    edited_count: "{n} cards edited",
    sessions: "{n} sessions",
    chars: "{n} chars",
    due: "due",
    priority: "priority",
    another_board: "another board",
    open_count: "{n} open",
    weekdays: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    months: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
};

pub static ES: Strings = Strings {
    title: "Resumen de actividad",
    no_activity: "Sin actividad en este período.",
    completed: "Completadas",
    started: "Iniciadas",
    created: "Creadas",
    moved: "Movidas",
    edited: "Editadas",
    deleted: "Eliminadas",
    archived: "Archivadas",
    external: "Cambiadas fuera de la app",
    pending: "Pendientes",
    overdue: "Vencidas",
    priorities: "Prioridades",
    weekend: "Durante el fin de semana",
    more: "+{n} más",
    edited_count: "{n} tarjetas editadas",
    sessions: "{n} sesiones",
    chars: "{n} caracteres",
    due: "vence",
    priority: "prioridad",
    another_board: "otro tablero",
    open_count: "{n} abiertas",
    weekdays: ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
    months: [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ],
};

pub static PT: Strings = Strings {
    title: "Resumo de atividade",
    no_activity: "Nenhuma atividade neste período.",
    completed: "Concluídos",
    started: "Iniciados",
    created: "Criados",
    moved: "Movidos",
    edited: "Editados",
    deleted: "Excluídos",
    archived: "Arquivados",
    external: "Alterados fora do app",
    pending: "Pendentes",
    overdue: "Atrasados",
    priorities: "Prioridades",
    weekend: "Durante o fim de semana",
    more: "+{n} mais",
    edited_count: "{n} cartões editados",
    sessions: "{n} sessões",
    chars: "{n} caracteres",
    due: "prazo",
    priority: "prioridade",
    another_board: "outro quadro",
    open_count: "{n} abertos",
    weekdays: ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"],
    months: [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ],
};

pub fn fill(template: &str, n: impl std::fmt::Display) -> String {
    template.replace("{n}", &n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_fill() {
        assert_eq!(Locale::parse("es-CL"), Locale::Es);
        assert_eq!(Locale::parse("pt"), Locale::Pt);
        assert_eq!(Locale::parse("fr"), Locale::En);
        assert_eq!(fill(Locale::Es.strings().more, 3), "+3 más");
    }
}
