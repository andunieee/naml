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

    /// the keys of the [Desktop Entry] group, in order
    pub fn entries(&self) -> Vec<(String, String)> {
        let mut in_main = false;
        let mut out = vec![];
        for line in &self.lines {
            let line = line.trim();
            if line.starts_with('[') {
                in_main = line == "[Desktop Entry]";
                continue;
            }
            if in_main
                && !line.starts_with('#')
                && let Some((k, v)) = line.split_once('=')
            {
                out.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        out
    }

    /// set a key of the [Desktop Entry] group, or remove it when `value` is `None`
    pub fn set(&mut self, key: &str, value: Option<&str>) {
        let mut in_main = false;
        let mut end_of_main = None;
        let mut found = false;
        let mut i = 0;
        while i < self.lines.len() {
            let line = self.lines[i].trim();
            if line.starts_with('[') {
                if in_main && end_of_main.is_none() {
                    end_of_main = Some(i);
                }
                in_main = line == "[Desktop Entry]";
                i += 1;
                continue;
            }
            if in_main && line.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
                match value {
                    Some(v) if !found => {
                        self.lines[i] = format!("{key}={v}");
                        found = true;
                    }
                    _ => {
                        self.lines.remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }
        if let (false, Some(v)) = (found, value) {
            let at = match end_of_main {
                Some(at) => at,
                None if in_main => self.lines.len(),
                None => {
                    self.lines.insert(0, "[Desktop Entry]".to_string());
                    1
                }
            };
            // keep the blank line that separates groups after the new key
            let at = (0..at)
                .rev()
                .take_while(|&j| self.lines[j].trim().is_empty())
                .last()
                .unwrap_or(at);
            self.lines.insert(at, format!("{key}={v}"));
        }
    }

    /// apply hand edits: "Key=value" sets a key, "-Key" removes it
    pub fn apply(&mut self, overrides: &[String]) {
        for o in overrides {
            match o.strip_prefix('-') {
                Some(key) => self.set(key, None),
                None => {
                    if let Some((k, v)) = o.split_once('=') {
                        self.set(k, Some(v));
                    }
                }
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
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
                    out.push(format!("X-Naml-Slug={slug}"));
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
                "X-Naml-Slug" => {}
                _ => out.push(line.clone()),
            }
        }
        if in_main {
            out.push(format!("X-Naml-Slug={slug}"));
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
        text.push_str(&format!("X-Naml-Slug={slug}\n"));
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
        assert_eq!(out.matches("X-Naml-Slug=foo").count(), 1);
        assert_eq!(d.get("Name").as_deref(), Some("Foo"));
    }

    #[test]
    fn applies_overrides() {
        let mut d = Desktop::parse(
            "[Desktop Entry]\nName=Foo\nComment=old\n\n[Desktop Action new]\nName=New\n",
        );
        d.apply(&[
            "Name=Bar".into(),
            "-Comment".into(),
            "Categories=Network;Chat;".into(),
        ]);
        assert_eq!(
            d.to_text(),
            "[Desktop Entry]\nName=Bar\nCategories=Network;Chat;\n\n[Desktop Action new]\nName=New\n"
        );
        assert_eq!(d.get("Name").as_deref(), Some("Bar"));
    }

    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(quote("/a b/c"), "\"/a b/c\"");
        assert_eq!(quote("/a/b"), "/a/b");
    }
}
