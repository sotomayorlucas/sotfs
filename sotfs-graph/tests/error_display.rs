//! Display + std::error::Error impl coverage for GraphError variants.
//!
//! These were sitting at 9% line coverage because nothing in the
//! workspace formatted the error to a string. The Display impl is
//! load-bearing for sotfsctl error reports, FUSE replies that include
//! a reason, and the prov-log `detail` field — making sure each arm
//! formats without panic catches typos and stale field references.

use sotfs_graph::GraphError;

fn d(e: GraphError) -> String {
    format!("{e}")
}

#[test]
fn all_variants_format() {
    let cases: Vec<(GraphError, &str)> = vec![
        (GraphError::InodeNotFound(7), "inode 7"),
        (GraphError::DirNotFound(8), "directory 8"),
        (GraphError::CapNotFound(9), "capability 9"),
        (GraphError::BlockNotFound(10), "block 10"),
        (GraphError::EdgeNotFound(11), "edge 11"),
        (
            GraphError::NameExists {
                dir: 1,
                name: "x".into(),
            },
            "'x' already exists",
        ),
        (GraphError::DirNotEmpty(1), "not empty"),
        (GraphError::LinkToDirectory(1), "hard-link to directory"),
        (GraphError::LinkCountExceeded(65535), "65535"),
        (GraphError::WouldCreateCycle, "directory cycle"),
        (GraphError::NameNotFound("a".into()), "'a' not found"),
        (GraphError::NotADirectory(2), "not a directory"),
        (GraphError::NotAFile(3), "not a regular file"),
        (GraphError::OutOfIds, "no free"),
        (GraphError::XAttrNotFound("user.x".into()), "user.x"),
        (GraphError::XAttrExists("user.y".into()), "user.y"),
        (GraphError::XAttrTooLarge(4096), "4096"),
        (GraphError::NotASymlink(4), "not a symlink"),
        (GraphError::SymlinkLoop, "too many levels"),
        (
            GraphError::QuotaExceeded {
                dir: 1,
                resource: "inodes".into(),
            },
            "inodes quota",
        ),
        (
            GraphError::InvariantViolation("G2: dangling".into()),
            "G2: dangling",
        ),
    ];
    for (err, needle) in cases {
        let s = d(err);
        assert!(s.contains(needle), "{s:?} should contain {needle:?}");
    }
}

#[test]
fn debug_impl_does_not_panic() {
    // Each variant has a Debug derive but if a future field is added
    // without updating Display this test will at least keep the field
    // non-trivial via the formatted output.
    let v = vec![
        GraphError::InodeNotFound(0),
        GraphError::WouldCreateCycle,
        GraphError::OutOfIds,
        GraphError::SymlinkLoop,
    ];
    for e in v {
        let s = format!("{e:?}");
        assert!(!s.is_empty());
    }
}

#[test]
fn graph_error_is_std_error() {
    fn assert_err<E: std::error::Error + Send + Sync + 'static>(_: &E) {}
    let e = GraphError::OutOfIds;
    assert_err(&e);
    // Source should be None for our variants — we don't wrap an inner
    // error at the moment.
    assert!(std::error::Error::source(&e).is_none());
}
