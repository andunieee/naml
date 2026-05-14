//! just enough of the desktop entry spec to read a few keys and point a launcher at our files

pub struct Desktop {
    lines: Vec<String>,
}

impl Desktop {
    pub fn parse(text: &str) -> Self {
        Self {
            lines: text.lines().map(|l| l.to_string()).collect(),
        }
    }

    /// a key from the [Desktop Entry] group
    pub fn get(&self, key: &str) -> Option<String> {
        let mut in_main = false;
        for line in &self.lines {
            let line = line.trim();
            if line.starts_with('[') {
                in_main = line == "[Desktop Entry]";
                continue;
            }
            if in_main
                && let Some((k, v)) = line.split_once('=')
                && k.trim() == key
            {
                return Some(v.trim().to_string());
            }
        }
        None
    }

    /// point Exec at `exec` (keeping arguments and field codes), Icon at `icon`, and tag the
    /// entry as ours
    pub fn rewrite(&self, exec: &str, icon: Option<&str>, slug: &str) -> String {
        let mut out = vec![];
        let mut in_main = false;
        for line in &self.lines {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                if in_main {
                    out.push(format!("X-Nam-Slug={slug}"));
                }
                in_main = trimmed == "[Desktop Entry]";
                out.push(line.clone());
                continue;
            }
            let Some((key, value)) = trimmed.split_once('=') else {
                out.push(line.clone());
                continue;
            };
            match key.trim() {
                "Exec" => {
                    let args = split_first_word(value.trim()).1;
                    if args.is_empty() {
                        out.push(format!("Exec={}", quote(exec)));
                    } else {
                        out.push(format!("Exec={} {}", quote(exec), args));
                    }
                }
                "TryExec" => out.push(format!("TryExec={exec}")),
                "Icon" => match icon {
                    Some(icon) => out.push(format!("Icon={icon}")),
                    None => out.push(line.clone()),
                },
                "X-Nam-Slug" => {}
                _ => out.push(line.clone()),
            }
        }
        if in_main {
            out.push(format!("X-Nam-Slug={slug}"));
        }
        let mut text = out.join("\n");
        text.push('\n');
        text
    }

    /// a minimal entry for apps that don't ship one
    pub fn minimal(
        name: &str,
        comment: Option<&str>,
        exec: &str,
        icon: Option<&str>,
        slug: &str,
    ) -> String {
        let mut text = format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={}\nTerminal=false\nCategories=Utility;\n",
            quote(exec)
        );
        if let Some(comment) = comment {
            text.push_str(&format!("Comment={comment}\n"));
        }
        if let Some(icon) = icon {
            text.push_str(&format!("Icon={icon}\n"));
        }
        text.push_str(&format!("X-Nam-Slug={slug}\n"));
        text
    }
}

/// the first word of an Exec value, which may be quoted, and the rest
fn split_first_word(value: &str) -> (&str, &str) {
    if let Some(rest) = value.strip_prefix('"') {
        let mut escaped = false;
        for (i, c) in rest.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => return (&rest[..i], rest[i + 1..].trim_start()),
                _ => escaped = false,
            }
        }
        return (rest, "");
    }
    match value.split_once(char::is_whitespace) {
        Some((first, rest)) => (first, rest.trim_start()),
        None => (value, ""),
    }
}

fn quote(path: &str) -> String {
    if path
        .chars()
        .any(|c| c.is_whitespace() || "\"'\\$`<>~|&;*?#()".contains(c))
    {
        let escaped = path
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`");
        format!("\"{escaped}\"")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_exec_and_icon() {
        let d = Desktop::parse(
            "[Desktop Entry]\nName=Foo\nExec=AppRun --no-sandbox %U\nIcon=foo\nTryExec=foo\n\n[Desktop Action new]\nExec=\"Foo App\" --new\n",
        );
        let out = d.rewrite(
            "/home/u/apps/foo/Foo.AppImage",
            Some("/icons/foo.png"),
            "foo",
        );
        assert!(out.contains("Exec=/home/u/apps/foo/Foo.AppImage --no-sandbox %U\n"));
        assert!(out.contains("Icon=/icons/foo.png\n"));
        assert!(out.contains("TryExec=/home/u/apps/foo/Foo.AppImage\n"));
        assert!(out.contains("Exec=/home/u/apps/foo/Foo.AppImage --new\n"));
        assert_eq!(out.matches("X-Nam-Slug=foo").count(), 1);
        assert_eq!(d.get("Name").as_deref(), Some("Foo"));
    }

    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(quote("/a b/c"), "\"/a b/c\"");
        assert_eq!(quote("/a/b"), "/a/b");
    }
}
