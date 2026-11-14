//! Fixes built into GSet for problems of single games. Profiles can only switch these on by name,
//! so a profile never carries code. Each one runs right before the game starts.

use std::path::Path;

pub struct Fix {
    pub id: &'static str,
    pub about: &'static str,
    run: fn(&FixContext) -> Result<Option<String>, String>,
}

/// What a fix gets to work with.
pub struct FixContext<'a> {
    /// the game's Wine prefix folder (Steam's STEAM_COMPAT_DATA_PATH), when Proton runs it
    pub compat_data: Option<&'a Path>,
    pub dry_run: bool,
}

pub const FIXES: &[Fix] = &[Fix {
    id: "nte-launcher-windows",
    about: "NTE’s launcher keeps a list of its windows in the prefix registry that grows by one at every \
            launch under Wine, and crashes at the 17th. This clears the list first.",
    run: nte_launcher_windows,
}];

pub fn known(id: &str) -> bool {
    FIXES.iter().any(|f| f.id == id)
}

/// Runs a fix. Ok(Some(text)) says what it changed, Ok(None) that there was nothing to do.
pub fn apply(id: &str, ctx: &FixContext) -> Result<Option<String>, String> {
    let fix = FIXES
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| format!("no fix called {id}"))?;
    (fix.run)(ctx)
}

/// HKCU\Software\PWRD\MobilePCGame\3000001: drop the "wnd" and "f" values.
fn nte_launcher_windows(ctx: &FixContext) -> Result<Option<String>, String> {
    let Some(compat) = ctx.compat_data else {
        return Ok(None);
    };
    let reg = compat.join("pfx/user.reg");
    let Ok(text) = std::fs::read_to_string(&reg) else {
        return Ok(None);
    };
    let (out, dropped) = drop_values(
        &text,
        r"[Software\\PWRD\\MobilePCGame\\3000001]",
        &["\"wnd\"=", "\"f\"="],
    );
    if dropped == 0 {
        return Ok(None);
    }
    if !ctx.dry_run {
        let tmp = reg.with_extension("reg.gset-tmp");
        std::fs::write(&tmp, out)
            .and_then(|()| std::fs::rename(&tmp, &reg))
            .map_err(|e| format!("{}: {e}", reg.display()))?;
    }
    Ok(Some(format!(
        "cleared NTE's window list ({dropped} values) in {}",
        reg.display()
    )))
}

/// Removes the lines starting with any of `values` inside the registry section whose header starts
/// with `section` (Wine writes the header followed by a timestamp).
fn drop_values(text: &str, section: &str, values: &[&str]) -> (String, usize) {
    let mut inside = false;
    let mut dropped = 0;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if line.starts_with('[') {
            inside = line.starts_with(section);
        } else if inside && values.iter().any(|v| line.starts_with(v)) {
            dropped += 1;
            continue;
        }
        out.push_str(line);
    }
    (out, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nte_fix_drops_only_its_values() {
        let reg = "WINE REGISTRY Version 2\n\n[Software\\\\Other] 1\n\"wnd\"=\"keep\"\n\n[Software\\\\PWRD\\\\MobilePCGame\\\\3000001] 1700000000\n#time=1\n\"f\"=dword:00000001\n\"wnd\"=hex:01,02\n\"other\"=\"keep\"\n";
        let (out, n) = drop_values(
            reg,
            r"[Software\\PWRD\\MobilePCGame\\3000001]",
            &["\"wnd\"=", "\"f\"="],
        );
        assert_eq!(n, 2);
        assert!(
            out.contains("\"wnd\"=\"keep\""),
            "values in other sections stay"
        );
        assert!(out.contains("\"other\"=\"keep\""));
        assert!(!out.contains("hex:01,02"));
    }

    #[test]
    fn fix_writes_the_registry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("pfx")).unwrap();
        let reg = dir.path().join("pfx/user.reg");
        std::fs::write(
            &reg,
            "[Software\\\\PWRD\\\\MobilePCGame\\\\3000001] 1\n\"wnd\"=hex:01\n",
        )
        .unwrap();
        let ctx = FixContext {
            compat_data: Some(dir.path()),
            dry_run: false,
        };
        assert!(apply("nte-launcher-windows", &ctx).unwrap().is_some());
        assert!(!std::fs::read_to_string(&reg).unwrap().contains("wnd"));
        assert!(
            apply("nte-launcher-windows", &ctx).unwrap().is_none(),
            "nothing left to do"
        );
    }
}
