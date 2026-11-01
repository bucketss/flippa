use std::path::{Path, PathBuf};

fn strip_suffix(game: &str) -> String {
    let g = game.trim_end_matches(['\\', '/']);
    for s in ["_downloads", "_addon"] {
        if g.to_lowercase().ends_with(s) {
            return g[..g.len() - s.len()].to_string();
        }
    }
    g.to_string()
}

pub fn search_dirs(bsp_path: &Path, game: Option<&Path>, extra: &[PathBuf]) -> Vec<PathBuf> {
    let bsp_path = std::path::absolute(bsp_path).unwrap_or_else(|_| bsp_path.to_path_buf());
    let bsp_dir = bsp_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let is_maps = bsp_dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("maps"));
    let game = if is_maps {
        bsp_dir.parent().map(Path::to_path_buf).unwrap_or_default()
    } else {
        game.map(Path::to_path_buf).unwrap_or_else(|| bsp_dir.parent().map(Path::to_path_buf).unwrap_or_default())
    };
    let game = PathBuf::from(strip_suffix(&game.to_string_lossy()));
    let root = game.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = game.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    let gs = game.to_string_lossy().into_owned();
    let mut dirs: Vec<PathBuf> = extra.to_vec();
    dirs.extend([
        bsp_dir.clone(),
        game.clone(),
        PathBuf::from(format!("{gs}_addon")),
        PathBuf::from(format!("{gs}_downloads")),
        root.join("valve"),
        root.join("valve_downloads"),
    ]);
    if name != "cstrike" && root.join("cstrike").is_dir() {
        dirs.push(root.join("cstrike"));
    }
    dirs
}
