use super::*;
use std::sync::Arc;

struct Collect(Mutex<Vec<CoreEvent>>);
impl EventSink for Collect {
    fn emit(&self, e: CoreEvent) {
        self.0.lock().push(e);
    }
}

fn core() -> (tempfile::TempDir, Arc<Core>, Arc<Collect>) {
    let d = tempfile::tempdir().unwrap();
    let sink = Arc::new(Collect(Mutex::new(vec![])));
    let c = Core::new(AppPaths::under(&d.path().join("app")), sink.clone()).unwrap();
    (d, c, sink)
}

fn lanes(snap: &BoardSnapshot) -> Vec<String> {
    snap.lanes.iter().map(|l| l.id.clone()).collect()
}

#[test]
fn create_open_apply_search() {
    let (d, c, sink) = core();
    let snap = c
        .create_board(
            &d.path().join("Work"),
            "Work",
            BoardKind::Kanban,
            &["To do".into(), "Done".into()],
            false,
        )
        .unwrap();
    assert_eq!(snap.lanes.len(), 2);
    let b = snap.header.id.clone();
    let k = lanes(&snap)[0].clone();
    let id = c.new_card_id();
    c.apply(
        &b,
        Op::CreateCard {
            id: id.clone(),
            parent: Parent::Lane(k),
            index: None,
            content: "# Fix relogin\n\n#backend\n".into(),
        },
        "New card",
        None,
    )
    .unwrap();
    assert!(
        sink.0
            .lock()
            .iter()
            .any(|e| matches!(e, CoreEvent::BoardDelta { .. }))
    );
    let hits = c.search("login", &SearchOptions::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, id);
    assert_eq!(hits[0].board_name, "Work");
    // Kind rules enforced at the app layer.
    let bad = c.apply(
        &b,
        Op::CreateCard {
            id: c.new_card_id(),
            parent: Parent::Root,
            index: None,
            content: "# x".into(),
        },
        "x",
        None,
    );
    assert!(bad.is_err());
    // Journal written.
    let h = c.history(&b, &HistoryFilter::default()).unwrap();
    assert!(h.iter().any(|e| e.kind == "createCard"));
    assert_eq!(c.registry().boards.len(), 1);
}

#[test]
fn edits_coalesce_into_one_history_version() {
    let (d, c, _) = core();
    let snap = c
        .create_board(
            &d.path().join("W"),
            "W",
            BoardKind::Kanban,
            &["L".into()],
            false,
        )
        .unwrap();
    let b = snap.header.id.clone();
    let id = c.new_card_id();
    c.apply(
        &b,
        Op::CreateCard {
            id: id.clone(),
            parent: Parent::Lane(lanes(&snap)[0].clone()),
            index: None,
            content: "# A\n".into(),
        },
        "n",
        None,
    )
    .unwrap();
    for i in 0..4 {
        c.write_card(&b, &id, &format!("# A\n\nv{i}\n"), Some("s1".into()))
            .unwrap();
    }
    c.seal(&b, Some(&id));
    let edits: Vec<_> = c
        .history(
            &b,
            &HistoryFilter {
                kinds: vec!["edit".into()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(edits.len(), 1);
    let before = c
        .history_blob(&b, edits[0].before.as_ref().unwrap())
        .unwrap();
    let after = c
        .history_blob(&b, edits[0].after.as_ref().unwrap())
        .unwrap();
    assert_eq!(before, "# A\n");
    assert_eq!(after, "# A\n\nv3\n");
    // One undo reverts the whole session.
    c.undo(&b).unwrap();
    assert_eq!(c.read_card(&b, &id).unwrap(), "# A\n");
}

#[test]
fn cross_board_move_with_undo_redo() {
    let (d, c, _) = core();
    let s1 = c
        .create_board(
            &d.path().join("A"),
            "A",
            BoardKind::Kanban,
            &["L".into()],
            false,
        )
        .unwrap();
    let s2 = c
        .create_board(&d.path().join("B"), "B", BoardKind::Files, &[], false)
        .unwrap();
    let (a, b) = (s1.header.id.clone(), s2.header.id.clone());
    let lane = lanes(&s1)[0].clone();
    let p = c.new_card_id();
    let ch = c.new_card_id();
    c.apply(
        &a,
        Op::CreateCard {
            id: p.clone(),
            parent: Parent::Lane(lane.clone()),
            index: None,
            content: "# Parent\n".into(),
        },
        "n",
        None,
    )
    .unwrap();
    c.apply(
        &a,
        Op::CreateCard {
            id: ch.clone(),
            parent: Parent::Card(p.clone()),
            index: None,
            content: "# Child\n".into(),
        },
        "n",
        None,
    )
    .unwrap();
    {
        let bs = c.board(&a).unwrap();
        let mut s = bs.lock();
        let (_, _) =
            files::add_attachment(&mut s, &ch, files::Source::Bytes(b"img"), "shot.png").unwrap();
    }
    c.move_across(&a, std::slice::from_ref(&p), &b, Parent::Root, None)
        .unwrap();
    {
        let sa = c.snapshot(&a).unwrap();
        let sb = c.snapshot(&b).unwrap();
        assert!(sa.nodes.is_empty());
        assert_eq!(sb.root_order, vec![p.clone()]);
        let child = sb.nodes.iter().find(|n| n.id == ch).unwrap();
        assert_eq!(child.attachments.len(), 1);
    }
    c.undo(&b).unwrap();
    assert_eq!(c.snapshot(&a).unwrap().lanes[0].order, vec![p.clone()]);
    assert!(c.snapshot(&b).unwrap().nodes.is_empty());
    c.redo(&b).unwrap();
    assert_eq!(c.snapshot(&b).unwrap().root_order, vec![p.clone()]);
    // Disk agrees after reopen.
    c.close_board(&b);
    let sb = c.open_board_by_id(&b).unwrap();
    assert_eq!(sb.nodes.len(), 2);
}

#[test]
fn external_changes_are_detected() {
    let (d, c, sink) = core();
    let snap = c
        .create_board(
            &d.path().join("W"),
            "W",
            BoardKind::Kanban,
            &["L".into()],
            false,
        )
        .unwrap();
    let b = snap.header.id.clone();
    let k = lanes(&snap)[0].clone();
    let file = d.path().join("W").join(&k).join("cext001.md");
    std::fs::write(&file, "# From another editor\n").unwrap();
    watch::process(&c, &b, &[file]);
    let s = c.snapshot(&b).unwrap();
    assert_eq!(s.nodes.len(), 1);
    assert!(
        sink.0
            .lock()
            .iter()
            .any(|e| matches!(e, CoreEvent::ExternalChange { .. }))
    );
    let h = c
        .history(
            &b,
            &HistoryFilter {
                kinds: vec!["externalEdit".into()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(
        c.search("another editor", &SearchOptions::default())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn nested_boards_are_rejected_and_unlinked_files_cleaned() {
    let (d, c, _) = core();
    let snap = c
        .create_board(
            &d.path().join("W"),
            "W",
            BoardKind::Kanban,
            &["L".into()],
            false,
        )
        .unwrap();
    assert!(
        c.create_board(
            &d.path().join("W/inner"),
            "In",
            BoardKind::Kanban,
            &[],
            false
        )
        .is_err()
    );
    let b = snap.header.id.clone();
    let id = c.new_card_id();
    c.apply(
        &b,
        Op::CreateCard {
            id: id.clone(),
            parent: Parent::Lane(lanes(&snap)[0].clone()),
            index: None,
            content: "# A\n".into(),
        },
        "n",
        None,
    )
    .unwrap();
    let att = c
        .add_attachment(&b, &id, files::Source::Bytes(b"x"), "keep.png")
        .unwrap();
    c.add_attachment(&b, &id, files::Source::Bytes(b"y"), "drop.png")
        .unwrap();
    c.write_card(&b, &id, &format!("# A\n\n![]({})\n", att.file), None)
        .unwrap();
    assert_eq!(c.cleanup_unlinked(&b, false).unwrap(), 0); // within TTL
    assert_eq!(c.cleanup_unlinked(&b, true).unwrap(), 1);
    let s = c.snapshot(&b).unwrap();
    assert_eq!(s.nodes[0].attachments.len(), 1);
    assert_eq!(s.nodes[0].attachments[0].file, att.file);
}
