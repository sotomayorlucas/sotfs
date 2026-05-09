//! Cargo run example: ejercita el grafo y valida el índice tras cada mutación.
use sotfs_graph::graph::TypeGraph;
use sotfs_graph::types::Permissions;
use sotfs_ops::{create_file, mkdir, rename, unlink};

fn main() {
    let mut g = TypeGraph::new();
    let rd = g.root_dir;
    let mut names: Vec<String> = (0..200).map(|i| format!("f{}", i)).collect();
    for n in &names {
        create_file(&mut g, rd, n, 0, 0, Permissions::FILE_DEFAULT).unwrap();
        g.check_dir_name_idx_consistency().expect("after create");
    }
    // mkdirs intercalados
    for i in 0..50 {
        let d = format!("d{}", i);
        mkdir(&mut g, rd, &d, 0, 0, Permissions::DIR_DEFAULT).unwrap();
        g.check_dir_name_idx_consistency().expect("after mkdir");
    }
    // renames
    for i in 0..200 {
        let new = format!("r{}", i);
        rename(&mut g, rd, &names[i], rd, &new).unwrap();
        names[i] = new;
        g.check_dir_name_idx_consistency().expect("after rename");
    }
    // unlinks
    for n in &names {
        unlink(&mut g, rd, n).unwrap();
        g.check_dir_name_idx_consistency().expect("after unlink");
    }
    println!("OK: 200 creates + 50 mkdirs + 200 renames + 200 unlinks, índice consistente");
}
