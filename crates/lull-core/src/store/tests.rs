use super::*;
use crate::ids::new_id;
use proptest::prelude::*;
use std::collections::BTreeMap;

fn cid() -> String {
    new_id(IdKind::Card, |_| false)
}
fn kid() -> String {
    new_id(IdKind::Lane, |_| false)
}

fn kanban() -> (tempfile::TempDir, BoardStore) {
    let d = tempfile::tempdir().unwrap();
    let s =
        BoardStore::create(d.path(), "b000001".into(), "Test".into(), BoardKind::Kanban).unwrap();
    (d, s)
}

fn lane(s: &mut BoardStore, name: &str) -> String {
    let k = kid();
    s.apply_user(
        Op::CreateLane {
            id: k.clone(),
            name: name.into(),
            index: None,
        },
        "lane",
        None,
    )
    .unwrap();
    k
}

fn card(s: &mut BoardStore, parent: Parent, title: &str) -> String {
    let c = cid();
    s.apply_user(
        Op::CreateCard {
            id: c.clone(),
            parent,
            index: None,
            content: format!("# {title}\n"),
        },
        "card",
        None,
    )
    .unwrap();
    c
}

/// Structural fingerprint used to compare memory vs disk.
#[allow(clippy::type_complexity)]
fn shape(
    st: &BoardState,
) -> (
    Vec<(String, String, Vec<String>, bool)>,
    BTreeMap<String, (Parent, bool, Vec<String>, bool, String)>,
) {
    let lanes = st
        .lanes
        .iter()
        .map(|l| (l.id.clone(), l.name.clone(), l.order.clone(), l.archived))
        .collect();
    let nodes = st
        .nodes
        .iter()
        .map(|(id, n)| {
            (
                id.clone(),
                (
                    n.parent.clone(),
                    n.is_group,
                    n.children.clone(),
                    n.archived,
                    n.meta.title.clone(),
                ),
            )
        })
        .collect();
    (lanes, nodes)
}

fn assert_disk_matches(s: &BoardStore) {
    let disk = load_board(&s.state.root, None).unwrap();
    assert_eq!(shape(&disk), shape(&s.state), "disk and memory diverged");
    assert_eq!(disk.root_order, s.state.root_order);
    assert!(disk.warnings.is_empty(), "warnings: {:?}", disk.warnings);
}

fn check_invariants(st: &BoardState) {
    // Every node appears exactly once in its parent's children.
    for (id, n) in &st.nodes {
        let siblings = st.children_of(&n.parent).expect("parent exists");
        assert_eq!(
            siblings.iter().filter(|c| *c == id).count(),
            1,
            "{id} not in parent"
        );
        // Groups are never empty.
        if n.is_group {
            assert!(!n.children.is_empty(), "empty group {id}");
        }
        assert!(n.is_group || n.children.is_empty());
        // Files exist where the model says.
        assert!(st.node_file(id).unwrap().exists(), "missing file for {id}");
    }
    let listed: usize = st.lanes.iter().map(|l| l.order.len()).sum::<usize>()
        + st.root_order.len()
        + st.nodes.values().map(|n| n.children.len()).sum::<usize>();
    assert_eq!(listed, st.nodes.len());
}

#[test]
fn create_board_and_cards() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "Backlog");
    let a = card(&mut s, Parent::Lane(k.clone()), "Alpha");
    let b = card(&mut s, Parent::Lane(k.clone()), "Beta");
    assert_eq!(s.state.lane(&k).unwrap().order, vec![a.clone(), b.clone()]);
    assert_eq!(s.state.nodes[&a].meta.title, "Alpha");
    assert!(s.state.root.join(&k).join(format!("{a}.md")).exists());
    assert_disk_matches(&s);
    check_invariants(&s.state);
}

#[test]
fn nesting_converts_to_group_and_back() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "L");
    let a = card(&mut s, Parent::Lane(k.clone()), "Parent");
    let b = card(&mut s, Parent::Lane(k.clone()), "Child");
    // Attachment on the parent must follow the conversion.
    let att = s.state.root.join(&k).join(format!("{a}.ab12-pic.png"));
    fs::write(&att, b"png").unwrap();
    s.rescan_attachments(&a);

    s.apply_user(
        Op::Move {
            ids: vec![b.clone()],
            to: Parent::Card(a.clone()),
            before: None,
        },
        "nest",
        None,
    )
    .unwrap();
    let n = &s.state.nodes[&a];
    assert!(n.is_group);
    assert_eq!(n.children, vec![b.clone()]);
    let gdir = s.state.root.join(&k).join(&a);
    assert!(gdir.join("index.md").exists());
    assert!(gdir.join(format!("{b}.md")).exists());
    assert!(gdir.join(format!("{a}.ab12-pic.png")).exists());
    assert_disk_matches(&s);

    // Move the child out: the group becomes a plain card again.
    s.apply_user(
        Op::Move {
            ids: vec![b.clone()],
            to: Parent::Lane(k.clone()),
            before: Some(a.clone()),
        },
        "out",
        None,
    )
    .unwrap();
    assert!(!s.state.nodes[&a].is_group);
    assert!(!gdir.exists());
    assert!(s.state.root.join(&k).join(format!("{a}.md")).exists());
    assert!(
        s.state
            .root
            .join(&k)
            .join(format!("{a}.ab12-pic.png"))
            .exists()
    );
    assert_eq!(s.state.lane(&k).unwrap().order, vec![b.clone(), a.clone()]);
    assert_disk_matches(&s);
    check_invariants(&s.state);
}

#[test]
fn undo_redo_roundtrip() {
    let (_d, mut s) = kanban();
    let k1 = lane(&mut s, "One");
    let k2 = lane(&mut s, "Two");
    let a = card(&mut s, Parent::Lane(k1.clone()), "A");
    let b = card(&mut s, Parent::Lane(k1.clone()), "B");
    let before = shape(&s.state);
    s.apply_user(
        Op::Move {
            ids: vec![a.clone()],
            to: Parent::Card(b.clone()),
            before: None,
        },
        "m1",
        None,
    )
    .unwrap();
    s.apply_user(
        Op::Move {
            ids: vec![b.clone()],
            to: Parent::Lane(k2.clone()),
            before: None,
        },
        "m2",
        None,
    )
    .unwrap();
    s.apply_user(
        Op::WriteCard {
            id: a.clone(),
            content: "# A2\n\nbody\n".into(),
        },
        "edit",
        None,
    )
    .unwrap();
    s.apply_user(
        Op::Trash {
            nodes: vec![b.clone()],
            lanes: vec![],
        },
        "del",
        None,
    )
    .unwrap();
    assert!(!s.state.nodes.contains_key(&a));
    let after = shape(&s.state);
    for _ in 0..4 {
        s.undo().unwrap().unwrap();
        check_invariants(&s.state);
        assert_disk_matches(&s);
    }
    assert_eq!(shape(&s.state), before);
    for _ in 0..4 {
        s.redo().unwrap().unwrap();
        check_invariants(&s.state);
    }
    assert_eq!(shape(&s.state), after);
    assert_disk_matches(&s);
}

#[test]
fn trash_and_restore_lane() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "Doomed");
    let a = card(&mut s, Parent::Lane(k.clone()), "A");
    let b = card(&mut s, Parent::Lane(k.clone()), "B");
    s.apply_user(
        Op::Move {
            ids: vec![b.clone()],
            to: Parent::Card(a.clone()),
            before: None,
        },
        "nest",
        None,
    )
    .unwrap();
    let before = shape(&s.state);
    let applied = s
        .apply_user(
            Op::Trash {
                nodes: vec![],
                lanes: vec![k.clone()],
            },
            "del lane",
            None,
        )
        .unwrap();
    assert!(s.state.lanes.is_empty());
    assert!(s.state.nodes.is_empty());
    assert_eq!(trash::list(&s.state.root).len(), 1);
    assert_eq!(applied.trashed.len(), 1);
    s.undo().unwrap();
    assert_eq!(shape(&s.state), before);
    assert!(trash::list(&s.state.root).is_empty());
    assert_disk_matches(&s);
}

#[test]
fn coalesced_edits_are_one_undo_step() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "L");
    let a = card(&mut s, Parent::Lane(k), "A");
    for i in 0..5 {
        s.apply_user(
            Op::WriteCard {
                id: a.clone(),
                content: format!("# A\n\nv{i}\n"),
            },
            "edit",
            Some(format!("sess:{a}")),
        )
        .unwrap();
    }
    s.undo().unwrap();
    assert_eq!(s.read_content(&a).unwrap(), "# A\n");
}

#[test]
fn cannot_move_into_descendant() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "L");
    let a = card(&mut s, Parent::Lane(k.clone()), "A");
    let b = card(&mut s, Parent::Card(a.clone()), "B");
    let r = s.apply_user(
        Op::Move {
            ids: vec![a.clone()],
            to: Parent::Card(b.clone()),
            before: None,
        },
        "bad",
        None,
    );
    assert!(r.is_err());
    check_invariants(&s.state);
}

#[test]
fn recovery_from_external_changes() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "L");
    let a = card(&mut s, Parent::Lane(k.clone()), "A");
    let dir = s.state.root.join(&k);
    // A card created by another editor is appended; a missing one is dropped.
    fs::write(dir.join("czzzzz9.md"), "# External\n").unwrap();
    fs::remove_file(dir.join(format!("{a}.md"))).unwrap();
    // Broken index.json is recovered from disk contents.
    fs::write(dir.join("index.json"), "{ not json").unwrap();
    let st = load_board(&s.state.root, None).unwrap();
    let l = st.lane(&k).unwrap();
    assert_eq!(l.order, vec!["czzzzz9".to_string()]);
    assert_eq!(st.nodes["czzzzz9"].meta.title, "External");
    assert!(st.warnings.iter().any(|w| w.starts_with("recovered:")));
    // Unknown files and non-id folders are ignored and untouched.
    fs::write(dir.join("notes.txt"), "keep me").unwrap();
    let st = load_board(&s.state.root, None).unwrap();
    assert_eq!(st.nodes.len(), 1);
    assert!(dir.join("notes.txt").exists());
}

#[test]
fn reload_reports_changes() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "L");
    let a = card(&mut s, Parent::Lane(k.clone()), "A");
    std::thread::sleep(std::time::Duration::from_millis(15));
    fs::write(
        s.state.root.join(&k).join(format!("{a}.md")),
        "# Changed title\n",
    )
    .unwrap();
    let ch = s.reload().unwrap();
    assert!(ch.nodes.contains(&a));
    assert_eq!(s.state.nodes[&a].meta.title, "Changed title");
}

#[test]
fn files_board_and_conversion() {
    let d = tempfile::tempdir().unwrap();
    let mut s =
        BoardStore::create(d.path(), "b000002".into(), "Notes".into(), BoardKind::Files).unwrap();
    let a = card(&mut s, Parent::Root, "Doc A");
    let b = card(&mut s, Parent::Root, "Doc B");
    let c = card(&mut s, Parent::Card(a.clone()), "Sub");
    assert_eq!(s.state.root_order, vec![a.clone(), b.clone()]);
    assert_disk_matches(&s);
    // Files -> Kanban: everything into an Inbox lane.
    let inbox = kid();
    let before = shape(&s.state);
    s.apply_user(
        Op::Batch {
            ops: vec![
                Op::SetKind {
                    kind: BoardKind::Kanban,
                },
                Op::CreateLane {
                    id: inbox.clone(),
                    name: "Inbox".into(),
                    index: None,
                },
                Op::Move {
                    ids: vec![a.clone(), b.clone()],
                    to: Parent::Lane(inbox.clone()),
                    before: None,
                },
            ],
        },
        "convert",
        None,
    )
    .unwrap();
    assert_eq!(
        s.state.lane(&inbox).unwrap().order,
        vec![a.clone(), b.clone()]
    );
    assert!(s.state.root_order.is_empty());
    assert_eq!(s.state.nodes[&c].parent, Parent::Card(a.clone()));
    assert_disk_matches(&s);
    s.undo().unwrap();
    assert_eq!(shape(&s.state), before);
    assert_eq!(s.state.manifest.kind, BoardKind::Files);
    assert_disk_matches(&s);
}

#[test]
fn archive_flags_persist() {
    let (_d, mut s) = kanban();
    let k = lane(&mut s, "L");
    let a = card(&mut s, Parent::Lane(k.clone()), "A");
    s.apply_user(
        Op::SetArchived {
            nodes: vec![(a.clone(), true)],
            lanes: vec![(k.clone(), true)],
        },
        "arch",
        None,
    )
    .unwrap();
    assert_disk_matches(&s);
    let st = load_board(&s.state.root, None).unwrap();
    assert!(st.nodes[&a].archived);
    assert!(st.lane(&k).unwrap().archived);
    s.undo().unwrap();
    assert!(!s.state.nodes[&a].archived);
}

#[test]
fn newer_schema_is_read_only() {
    let (_d, mut s) = kanban();
    let mp = manifest_path(&s.state.root);
    let text = fs::read_to_string(&mp)
        .unwrap()
        .replace("\"schema\": 1", "\"schema\": 99");
    fs::write(&mp, text).unwrap();
    let mut s2 = BoardStore::open(&s.state.root).unwrap();
    assert!(s2.state.read_only.is_some());
    assert!(
        s2.apply(Op::CreateLane {
            id: kid(),
            name: "x".into(),
            index: None
        })
        .is_err()
    );
    let _ = &mut s;
}

// --- property-based: random operation sequences --------------------------

#[derive(Debug, Clone)]
enum Action {
    NewLane,
    NewCard(usize, Option<usize>),
    Move(usize, usize, bool),
    Trash(usize),
    Edit(usize),
    Archive(usize),
    MoveLane(usize, usize),
    Undo,
    Redo,
}

fn action() -> impl Strategy<Value = Action> {
    prop_oneof![
        1 => Just(Action::NewLane),
        4 => (any::<usize>(), proptest::option::of(any::<usize>())).prop_map(|(a, b)| Action::NewCard(a, b)),
        5 => (any::<usize>(), any::<usize>(), any::<bool>()).prop_map(|(a, b, c)| Action::Move(a, b, c)),
        1 => any::<usize>().prop_map(Action::Trash),
        2 => any::<usize>().prop_map(Action::Edit),
        1 => any::<usize>().prop_map(Action::Archive),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Action::MoveLane(a, b)),
        2 => Just(Action::Undo),
        1 => Just(Action::Redo),
    ]
}

fn pick<T: Clone>(v: &[T], i: usize) -> Option<T> {
    (!v.is_empty()).then(|| v[i % v.len()].clone())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, .. ProptestConfig::default() })]
    #[test]
    fn random_ops_keep_invariants(actions in proptest::collection::vec(action(), 1..40)) {
        let (_d, mut s) = kanban();
        lane(&mut s, "Start");
        for a in actions {
            let mut ids: Vec<String> = s.state.nodes.keys().cloned().collect();
            ids.sort();
            let lanes: Vec<String> = s.state.lanes.iter().map(|l| l.id.clone()).collect();
            let _ = match a {
                Action::NewLane => s.apply_user(Op::CreateLane { id: kid(), name: "L".into(), index: None }, "l", None).map(|_| ()),
                Action::NewCard(p, into) => {
                    let parent = match into.and_then(|i| pick(&ids, i)) {
                        Some(c) => Parent::Card(c),
                        None => match pick(&lanes, p) { Some(k) => Parent::Lane(k), None => continue },
                    };
                    s.apply_user(Op::CreateCard { id: cid(), parent, index: None, content: "# c\n".into() }, "c", None).map(|_| ())
                }
                Action::Move(x, y, into_card) => {
                    let Some(id) = pick(&ids, x) else { continue };
                    let to = if into_card {
                        match pick(&ids, y) { Some(c) => Parent::Card(c), None => continue }
                    } else {
                        match pick(&lanes, y) { Some(k) => Parent::Lane(k), None => continue }
                    };
                    let before = pick(&ids, y.wrapping_add(1));
                    s.apply_user(Op::Move { ids: vec![id], to, before }, "m", None).map(|_| ())
                }
                Action::Trash(x) => match pick(&ids, x) {
                    Some(id) => s.apply_user(Op::Trash { nodes: vec![id], lanes: vec![] }, "t", None).map(|_| ()),
                    None => continue,
                },
                Action::Edit(x) => match pick(&ids, x) {
                    Some(id) => s.apply_user(Op::WriteCard { id, content: format!("# e{x}\n\n- [ ] t\n") }, "e", None).map(|_| ()),
                    None => continue,
                },
                Action::Archive(x) => match pick(&ids, x) {
                    Some(id) => s.apply_user(Op::SetArchived { nodes: vec![(id, x % 2 == 0)], lanes: vec![] }, "a", None).map(|_| ()),
                    None => continue,
                },
                Action::MoveLane(x, y) => match pick(&lanes, x) {
                    Some(k) => s.apply_user(Op::MoveLane { id: k, index: y % 4 }, "ml", None).map(|_| ()),
                    None => continue,
                },
                Action::Undo => s.undo().map(|_| ()),
                Action::Redo => s.redo().map(|_| ()),
            };
            check_invariants(&s.state);
        }
        assert_disk_matches(&s);
    }
}
