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

#[test]
fn long_edit_session_flush_does_not_deadlock() {
    // Regression: after 60s of continuous typing, `apply` flushed the pending
    // edit while holding the board lock and then re-locked the same board.
    let (d, c, _) = core();
    let snap = c
        .create_board(
            &d.path().join("W"),
            "W",
            BoardKind::Kanban,
            &["A".into()],
            false,
        )
        .unwrap();
    let b = snap.header.id.clone();
    let id = c.new_card_id();
    c.apply(
        &b,
        Op::CreateCard {
            id: id.clone(),
            parent: Parent::Lane(snap.lanes[0].id.clone()),
            index: None,
            content: "# a\n".into(),
        },
        "x",
        None,
    )
    .unwrap();
    c.apply(
        &b,
        Op::WriteCard {
            id: id.clone(),
            content: "# a1\n".into(),
        },
        "Edit",
        Some("s".into()),
    )
    .unwrap();
    {
        let mut p = c.pending.lock();
        let e = p.get_mut(&(b.clone(), id.clone())).expect("pending edit");
        e.started = Instant::now() - std::time::Duration::from_secs(61);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let (c2, b2, id2) = (c.clone(), b.clone(), id.clone());
    std::thread::spawn(move || {
        let r = c2.apply(
            &b2,
            Op::WriteCard {
                id: id2,
                content: "# a2\n".into(),
            },
            "Edit",
            Some("s".into()),
        );
        tx.send(r.is_ok()).ok();
    });
    let ok = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("apply deadlocked");
    assert!(ok);
    // The old session was journaled as its own version.
    let h = c.history(&b, &HistoryFilter::default()).unwrap();
    assert!(h.iter().any(|e| e.kind == "edit"));
}

#[test]
fn concurrent_undo_redo_never_panics() {
    let (d, c, _) = core();
    let snap = c
        .create_board(
            &d.path().join("U"),
            "U",
            BoardKind::Kanban,
            &["A".into()],
            false,
        )
        .unwrap();
    let b = snap.header.id.clone();
    let lane = snap.lanes[0].id.clone();
    for i in 0..40 {
        c.apply(
            &b,
            Op::CreateCard {
                id: c.new_card_id(),
                parent: Parent::Lane(lane.clone()),
                index: None,
                content: format!("# {i}\n"),
            },
            "New",
            None,
        )
        .unwrap();
    }
    let hs: Vec<_> = (0..6)
        .map(|k| {
            let (c, b) = (c.clone(), b.clone());
            std::thread::spawn(move || {
                for _ in 0..30 {
                    let _ = if k % 2 == 0 { c.undo(&b) } else { c.redo(&b) };
                }
            })
        })
        .collect();
    for h in hs {
        h.join().expect("undo/redo thread panicked");
    }
    // Stacks stay consistent: the remaining steps can still be undone.
    while c.undo(&b).unwrap().done {}
    assert!(c.snapshot(&b).unwrap().lanes[0].order.is_empty());
}

#[test]
fn failed_undo_keeps_its_step_when_transient() {
    let (d, c, _) = core();
    let snap = c
        .create_board(
            &d.path().join("R"),
            "R",
            BoardKind::Kanban,
            &["A".into()],
            false,
        )
        .unwrap();
    let b = snap.header.id.clone();
    let id = c.new_card_id();
    c.apply(
        &b,
        Op::CreateCard {
            id: id.clone(),
            parent: Parent::Lane(snap.lanes[0].id.clone()),
            index: None,
            content: "# x\n".into(),
        },
        "New",
        None,
    )
    .unwrap();
    let store = c.board(&b).unwrap();
    store.lock().state.read_only = Some("test".into());
    assert!(c.undo(&b).is_err());
    store.lock().state.read_only = None;
    assert!(c.undo(&b).unwrap().done, "step was kept");
    assert!(c.snapshot(&b).unwrap().nodes.is_empty());
}

#[test]
fn sweep_keeps_attachments_referenced_by_html_luau_urls_or_other_cards() {
    let (d, c, _) = core();
    let snap = c
        .create_board(
            &d.path().join("S"),
            "S",
            BoardKind::Kanban,
            &["A".into()],
            false,
        )
        .unwrap();
    let b = snap.header.id.clone();
    let lane = lanes(&snap)[0].clone();
    let mk = |content: &str| {
        let id = c.new_card_id();
        c.apply(
            &b,
            Op::CreateCard {
                id: id.clone(),
                parent: Parent::Lane(lane.clone()),
                index: None,
                content: content.into(),
            },
            "n",
            None,
        )
        .unwrap();
        id
    };
    let a = mk("# A\n");
    let other = mk("# Other\n");
    let html = c
        .add_attachment(&b, &a, files::Source::Bytes(b"1"), "html.png")
        .unwrap();
    let url = c
        .add_attachment(&b, &a, files::Source::Bytes(b"2"), "url pic.png")
        .unwrap();
    let cross = c
        .add_attachment(&b, &a, files::Source::Bytes(b"3"), "cross.pdf")
        .unwrap();
    c.add_attachment(&b, &a, files::Source::Bytes(b"4"), "unused.png")
        .unwrap();
    c.write_card(
        &b,
        &a,
        &format!("# A\n\n<img src=\"{}\" width=200>\n", html.file),
        None,
    )
    .unwrap();
    let enc = url.file.replace(' ', "%20");
    c.write_card(
        &b,
        &other,
        &format!(
            "# Other\n\nluau://localhost/{b}/{enc}\n[see]({})\n",
            cross.file
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        c.cleanup_unlinked(&b, true).unwrap(),
        1,
        "only the unused file moves"
    );
    let s = c.snapshot(&b).unwrap();
    let n = s.nodes.iter().find(|n| n.id == a).unwrap();
    let files: Vec<&str> = n.attachments.iter().map(|x| x.file.as_str()).collect();
    for f in [&html.file, &url.file, &cross.file] {
        assert!(files.contains(&f.as_str()), "{f} kept");
    }
}

#[test]
fn kind_rules_cover_place_and_batches_and_copies_need_new_ids() {
    let (d, c, _) = core();
    let root = d.path().join("K");
    let snap = c
        .create_board(&root, "K", BoardKind::Kanban, &["A".into()], false)
        .unwrap();
    let b = snap.header.id.clone();
    let id = c.new_card_id();
    c.apply(
        &b,
        Op::CreateCard {
            id: id.clone(),
            parent: Parent::Lane(snap.lanes[0].id.clone()),
            index: None,
            content: "# x\n".into(),
        },
        "n",
        None,
    )
    .unwrap();
    let to_root = Op::Place {
        items: vec![store::Placement {
            id: id.clone(),
            parent: Parent::Root,
            index: 0,
        }],
    };
    assert!(c.apply(&b, to_root.clone(), "p", None).is_err());
    assert!(
        c.apply(
            &b,
            Op::Batch {
                ops: vec![Op::Batch { ops: vec![to_root] }]
            },
            "b",
            None
        )
        .is_err()
    );
    // A copied board folder with the same id cannot replace the open one.
    let copy = d.path().join("K copy");
    std::fs::create_dir_all(copy.join(".luau")).unwrap();
    std::fs::copy(root.join(".luau/board.json"), copy.join(".luau/board.json")).unwrap();
    let e = c.open_board(&copy).unwrap_err();
    assert!(matches!(e, Error::Conflict(ref m) if m.starts_with("duplicate_board_id")));
    assert_eq!(c.board_root(&b).unwrap(), root);
}

#[test]
fn external_edits_are_not_overwritten_or_undone_away() {
    let (d, c, _) = core();
    let root = d.path().join("E");
    let snap = c
        .create_board(&root, "E", BoardKind::Kanban, &["A".into()], false)
        .unwrap();
    let b = snap.header.id.clone();
    let lane = snap.lanes[0].id.clone();
    let id = c.new_card_id();
    c.apply(
        &b,
        Op::CreateCard {
            id: id.clone(),
            parent: Parent::Lane(lane.clone()),
            index: None,
            content: "# A\n".into(),
        },
        "n",
        None,
    )
    .unwrap();
    c.write_card(&b, &id, "# A\n\nmine\n", None).unwrap();
    let file = root.join(&lane).join(format!("{id}.md"));
    // Our own write is an echo; the same path changed by someone else is not.
    {
        let s = c.board(&b).unwrap();
        assert!(s.lock().recently_touched(&file));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&file, "# A\n\ntheirs, longer\n").unwrap();
        assert!(
            !s.lock().recently_touched(&file),
            "external save right after ours"
        );
        // The reload drops undo steps for the card: undo cannot clobber it.
        s.lock().reload_external().unwrap();
        assert!(
            s.lock()
                .peek_undo()
                .is_none_or(|e| !matches!(&e.op, Op::WriteCard { id: x, .. } if *x == id))
        );
    }
    // A stale editor base is refused.
    let e = c
        .write_card_checked(&b, &id, "# A\n\nmine again\n", None, Some("# A\n\nmine\n"))
        .unwrap_err();
    assert!(matches!(e, Error::Conflict(ref m) if m == "changed_on_disk"));
    c.write_card_checked(
        &b,
        &id,
        "# A\n\nmerged\n",
        None,
        Some("# A\n\ntheirs, longer\n"),
    )
    .unwrap();
    assert!(c.read_card(&b, &id).unwrap().contains("merged"));
}

#[test]
fn template_links_cards_and_writes_assets() {
    use crate::io::templates::{BoardTemplate, TemplateAsset, TemplateLane};
    use base64::Engine;
    let (d, c, _) = core();
    let tpl = BoardTemplate {
        kind: BoardKind::Kanban,
        lanes: vec![
            TemplateLane {
                name: "Start".into(),
                cards: vec![
                    "# One\n\nSee [[{{card:1}}]] and ![pic]({{asset:pic.svg}}) {{other}}\n".into(),
                ],
            },
            TemplateLane {
                name: "Next".into(),
                cards: vec!["# Two\n".into()],
            },
        ],
        notes: vec![],
        assets: vec![TemplateAsset {
            card: 0,
            name: "pic.svg".into(),
            data: base64::engine::general_purpose::STANDARD.encode("<svg/>"),
        }],
    };
    let snap = c
        .create_from_template(&d.path().join("Guide"), "Guide", &tpl, false)
        .unwrap();
    let one = snap.nodes.iter().find(|n| n.title == "One").unwrap();
    let two = snap.nodes.iter().find(|n| n.title == "Two").unwrap();
    let text = c.read_card(&snap.header.id, &one.id).unwrap();
    assert!(text.contains(&format!("[[{}]]", two.id)), "{text}");
    assert!(text.contains("{{other}}"));
    assert_eq!(one.attachments.len(), 1);
    let file = &one.attachments[0].file;
    assert!(file.starts_with(&format!("{}.", one.id)) && file.ends_with("-pic.svg"));
    assert!(text.contains(&format!("]({file})")));
    let lane_dir = d.path().join("Guide").join(&snap.lanes[0].id);
    assert_eq!(
        std::fs::read_to_string(lane_dir.join(file)).unwrap(),
        "<svg/>"
    );

    // Assets must point at a template card and stay small.
    let mut bad = tpl.clone();
    bad.assets[0].card = 5;
    assert!(
        c.create_from_template(&d.path().join("Bad"), "Bad", &bad, false)
            .is_err()
    );
    bad.assets[0].card = 0;
    bad.assets[0].name = "../x.svg".into();
    assert!(
        c.create_from_template(&d.path().join("Bad2"), "Bad2", &bad, false)
            .is_err()
    );
}
