use std::path::{Path};

pub fn display_path(path: &Path, max_len: usize) -> String {
    let display = path.display();
    if display.to_string().len() <= max_len {
        return display.to_string()
    }

    let mut parts = to_string_components(path) ;
    let mut path_len = parts.iter().map(|p| p.len() + 1).sum::<usize>() - 1;

    while path_len > max_len && parts.len() > 3 {
        let middle = parts.len() / 2;
        let middle_len = get_part_len(middle, &parts);
        parts.remove(middle);
        path_len -= middle_len;
    }

    let mut result = parts.join("/");
    if result.len() > max_len {
        result = format!(
            "{}…{}", 
            &result[..(max_len / 2)], 
            &result[result.len().saturating_sub(max_len / 2)..]
        );
    }

    result
}

fn get_part_len(i: usize, parts: &[String]) -> usize {
    parts.get(i)
        .map(|el| el.len()+1)
        .unwrap_or(0)
} 

fn to_string_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|el| el.as_os_str().to_string_lossy().into_owned())
        .collect()
}