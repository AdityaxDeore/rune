//! Appending one row to a long `virtual_rows` list must not re-read every row's key:
//! with an unchanged `virtual_rows_version` and a longer list, the commit reads only
//! the new keys (plus a first/last guard), so an append costs O(new), not O(rows).
use std::sync::atomic::{AtomicUsize, Ordering};

use rune::{Cx, El, Env, GrowthSpec, Runtime, ScrollSpec, col, text};

const ROWS: usize = 5_000;

struct S {
    rows: Vec<String>,
    key_calls: AtomicUsize,
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    col().w_full().h_full().child(
        col()
            .key("viewport")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(
                col()
                    .key("rows")
                    .w_full()
                    .growth(GrowthSpec::default())
                    .virtual_rows(
                        s.rows.len(),
                        24.0,
                        |s: &S, i| {
                            s.key_calls.fetch_add(1, Ordering::Relaxed);
                            s.rows[i].as_str().into()
                        },
                        |s: &S, i| {
                            col()
                                .key(format!("r{i}"))
                                .w_full()
                                .h(24.0)
                                .child(text(s.rows[i].clone()))
                        },
                    )
                    .virtual_rows_version(0),
            ),
    )
}

fn calls(rt: &Runtime<S>) -> usize {
    rt.state.key_calls.load(Ordering::Relaxed)
}

#[test]
fn appending_one_row_reads_only_the_new_key() {
    let mut rt = Runtime::new(
        S {
            rows: (0..ROWS).map(|i| format!("r{i}")).collect(),
            key_calls: AtomicUsize::new(0),
        },
        view,
        (400.0, 300.0),
        Env::new(true, 1.0),
    );
    for _ in 0..10 {
        rt.frame(1.0 / 120.0);
    }
    rt.settle_all();

    // Append one row at the end, keeping the version: the old rows keep their keys.
    rt.state.key_calls.store(0, Ordering::Relaxed);
    rt.update(|s, _| s.rows.push(format!("r{ROWS}")));
    rt.frame(1.0 / 120.0);
    let appended_calls = calls(&rt);
    assert!(
        appended_calls < 100,
        "appending 1 row to {ROWS} called the key function {appended_calls} times; expected O(new)"
    );

    // The appended row is real: scrolling to it mounts it at the end of the list.
    // A settle frame mounts the whole overscan window at once (no per-frame budget).
    rt.update(|_, tx| tx.scroll_into_view(format!("r{ROWS}")));
    for _ in 0..10 {
        rt.frame(1.0 / 120.0);
    }
    rt.settle_all();
    rt.frame(1.0 / 120.0);
    let last = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(format!("r{ROWS}").as_str()))
        .expect("the appended row must mount when scrolled into view");
    assert!(
        last.visible[3] > 23.0,
        "appended row must show whole at the end of the list, visible height is {}",
        last.visible[3]
    );
}
