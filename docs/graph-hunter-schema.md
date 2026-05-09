# Graph Hunter export schema (`v1`)

sotFS exports its TypeGraph as a **temporal multigraph** JSON document.
Threat-hunting tools (APTHunter, PROGRAPHER, the GraphHunter component
of [sotX](https://github.com/sotomayorlucas/sotX), or any custom
consumer) can replay the events, build the live graph, and run pattern
detection over it.

The format is produced by `sotfs_graph::export::to_graph_hunter` and the
binary `sotfs-export-hunter`. It is part of the **public stable API of
sotFS**: any breaking change requires a SemVer major bump and a parallel
`v2` exporter so consumers can migrate.

## Document shape

```jsonc
{
  "meta": {
    "format": "graph-hunter-temporal",
    "version": 1,
    "node_count": 9,
    "edge_count": 10,
    "node_types":  ["inode", "directory", "capability", "block", "version"],
    "edge_types":  ["contains", "grants", "delegates", "derivesFrom",
                    "supersedes", "pointsTo", "hasXattr"]
  },
  "events": [
    { "t": 1778345083, "op": "add_node", "id": "I1", "type": "inode",
      "vtype": "dir", "size": 0, "link_count": 1, "perms": 493,
      "uid": 0, "gid": 0 },
    { "t": 1778345083, "op": "add_edge", "id": 1, "src": "D1",
      "tgt": "I1", "type": "contains", "name": "." }
    // ...
  ]
}
```

### `meta`

| Field         | Type     | Meaning                                        |
|---------------|----------|------------------------------------------------|
| `format`      | string   | Always `"graph-hunter-temporal"`               |
| `version`     | integer  | `1` for this schema; future versions will bump |
| `node_count`  | integer  | Total nodes emitted in `events`                |
| `edge_count`  | integer  | Total edges emitted                            |
| `node_types`  | string[] | Closed enum of node types in this document    |
| `edge_types`  | string[] | Closed enum of edge types in this document    |

### `events`

A flat array, ordered by `t` ascending then by emission order within the
same timestamp. Each event is one of:

#### `add_node`

| Field          | Required | Type   | Notes                                       |
|----------------|----------|--------|---------------------------------------------|
| `t`            | yes      | u64    | Unix timestamp (seconds) when the object was created |
| `op`           | yes      | string | Always `"add_node"`                         |
| `id`           | yes      | string | Stable id with a 1-letter type prefix:<br>`I…` inode, `D…` directory, `C…` capability, `B…` block, `V…` version |
| `type`         | yes      | string | One of `meta.node_types`                    |
| `vtype`        | inode-only | string | `"file"`, `"dir"`, `"symlink"`, `"chardev"`, `"blockdev"` |
| `size`         | inode-only | u64    | File size in bytes                          |
| `link_count`   | inode-only | u32    | POSIX hard-link count                       |
| `perms`        | inode-only | u16    | Mode bits (`0o755` etc.)                    |
| `uid`/`gid`    | inode-only | u32    | Owner                                       |
| `inode_id`     | dir-only | u64    | The inode this directory binds to           |
| `rights`       | cap-only | u8     | bitmask: `R=0x01`, `W=0x02`, `X=0x04`, `G=0x08`, `R=0x10` |
| `epoch`        | cap-only | u64    | Generation, advances on revoke              |

#### `add_edge`

| Field    | Required | Type   | Notes                                  |
|----------|----------|--------|----------------------------------------|
| `t`      | yes      | u64    | Same convention as nodes               |
| `op`     | yes      | string | Always `"add_edge"`                    |
| `id`     | yes      | u64    | EdgeId                                  |
| `src`    | yes      | string | Source node id (with prefix)            |
| `tgt`    | yes      | string | Target node id                          |
| `type`   | yes      | string | One of `meta.edge_types`               |
| `name`   | contains-only | string | The path component                    |

#### `remove_node` / `remove_edge` (streaming mode only)

Identical shape but `op = "remove_node"` / `"remove_edge"`. Snapshot
exports never emit these because they capture a single point in time.
Streaming exports (`sotfs-export-hunter --tail`) emit them as the graph
mutates.

## Determinism

Within a single snapshot `to_graph_hunter` call, the order of events is
deterministic for a given `TypeGraph`:

1. All `add_node` for nodes with the lowest creation time first, ordered
   by id within the same timestamp.
2. Then `add_edge` for edges sorted by their EdgeId (which is also
   monotonically increasing).

This makes diffing two consecutive snapshots a stable line-diff.

## Compatibility window

The `version: 1` schema is **stable**. The following changes are
considered backwards-compatible (consumers should ignore unknown keys):

- Adding new fields to existing event types.
- Adding new `node_types` / `edge_types` (consumers should ignore
  unknown types).
- Adding new event `op`s (consumers should ignore unknown ops).

The following are **breaking** and require `version: 2`:

- Renaming or removing a field.
- Changing the type or units of an existing field.
- Removing an `op`.

The CHANGELOG entry on a breaking change must include a migration note
and a parallel `v1` exporter (`sotfs_graph::export::to_graph_hunter_v1`)
must remain available for at least one major release.

## Reference implementation

- Producer: [`sotfs-graph/src/export.rs::to_graph_hunter`](../sotfs-graph/src/export.rs)
- Binary: [`sotfs-cli/src/bin/export_hunter.rs`](../sotfs-cli/src/bin/export_hunter.rs)
- Test: [`sotfs-graph/tests/graph_hunter_export.rs`](../sotfs-graph/tests/graph_hunter_export.rs)

## Example sequence

Mounting a fresh volume, creating two directories and a symlink, then
exporting yields events along these lines (timestamps trimmed):

```
add_node I1 type=inode vtype=dir size=0 link_count=1 perms=493
add_node D1 type=directory inode_id=1
add_edge 1 src=D1 tgt=I1 type=contains name="."
add_node I2 type=inode vtype=dir ...
add_edge 2 src=D1 tgt=I2 type=contains name="dir1"
add_node I6 type=inode vtype=symlink size=13 ...
add_edge 10 src=D3 tgt=I6 type=contains name="link-to-a"
```

Tools that build a property-graph live (e.g. PROGRAPHER) can ingest this
directly. Tools that expect OpenLineage / W3C PROV / DARPA E5 schemas
need an adapter layer; a `sotfs-hunter-adapter` crate is on the
post-`v0.2.0` roadmap.
