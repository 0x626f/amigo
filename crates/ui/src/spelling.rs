//! Local English, Spanish, and Russian spelling backed by installed Hunspell dictionaries.
mod personal;
pub use personal::{
    add_to_dictionary, is_personal_word, personal_revision, personal_words, set_personal_dictionary,
};
/// Apply the current personal dictionary to a manual document or word check.
pub fn check_in_context(
    text: &str,
    ctx: &egui::Context,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut issues = check(text)?;
    let accepted = personal_words(ctx);
    issues.retain(|(word, _)| !accepted.contains(&word.to_lowercase()));
    Ok(issues)
}
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::OnceLock,
};

pub fn check(text: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let dictionaries = dictionaries();
    let words: BTreeSet<_> = text
        .split(|c: char| !c.is_alphabetic() && !matches!(c, '\'' | '’' | '-'))
        .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()))
        .filter(|word| !word.is_empty())
        .collect();
    let (russian, latin): (Vec<_>, Vec<_>) = words
        .into_iter()
        .partition(|word| word.chars().any(is_cyrillic));
    let mut issues = Vec::new();
    if !latin.is_empty() {
        let input = latin.join("\n");
        let names: Vec<_> = dictionaries
            .iter()
            .filter(|d| !d.starts_with("ru"))
            .collect();
        let reports = if names.is_empty() {
            vec![run(&input, None)?]
        } else {
            names
                .into_iter()
                .map(|name| run(&input, Some(name)))
                .collect::<Result<Vec<_>, _>>()?
        };
        issues.extend(merge_reports(reports));
    }
    if !russian.is_empty() {
        let dictionary = dictionaries
            .iter()
            .find(|d| d.starts_with("ru"))
            .map(String::as_str);
        issues.extend(run(&russian.join("\n"), dictionary)?);
    }
    Ok(issues)
}
fn is_cyrillic(c: char) -> bool {
    matches!(c,'\u{0400}'..='\u{052f}'|'\u{2de0}'..='\u{2dff}'|'\u{a640}'..='\u{a69f}')
}
fn dictionaries() -> &'static Vec<String> {
    static DICTIONARIES: OnceLock<Vec<String>> = OnceLock::new();
    DICTIONARIES.get_or_init(|| {
        std::process::Command::new("hunspell")
            .args(["-D", "-a"])
            .stdin(std::process::Stdio::null())
            .output()
            .map(|out| {
                select_dictionaries(&format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ))
            })
            .unwrap_or_default()
    })
}
fn select_dictionaries(output: &str) -> Vec<String> {
    let names: BTreeSet<_> = output
        .lines()
        .filter_map(|line| Path::new(line.trim()).file_stem().and_then(|s| s.to_str()))
        .collect();
    [
        ["en_US", "en_GB", "en"],
        ["es_ES", "es_MX", "es"],
        ["ru_RU", "ru", "ru_UA"],
    ]
    .into_iter()
    .filter_map(|choices| {
        choices
            .into_iter()
            .find(|name| names.contains(name))
            .map(str::to_owned)
    })
    .collect()
}
/// A word is wrong only if every applicable language rejects it. Interleave their suggestions.
fn merge_reports(reports: Vec<Vec<(String, Vec<String>)>>) -> Vec<(String, Vec<String>)> {
    let reports: Vec<BTreeMap<_, _>> = reports
        .into_iter()
        .map(|issues| issues.into_iter().collect())
        .collect();
    let Some(first) = reports.first() else {
        return Vec::new();
    };
    first
        .keys()
        .filter(|word| reports.iter().all(|report| report.contains_key(*word)))
        .map(|word| {
            let mut suggestions = Vec::new();
            let longest = reports.iter().map(|r| r[word].len()).max().unwrap_or(0);
            for rank in 0..longest {
                for report in &reports {
                    if let Some(suggestion) = report[word].get(rank)
                        && !suggestions.contains(suggestion)
                    {
                        suggestions.push(suggestion.clone());
                    }
                    if suggestions.len() == 6 {
                        return (word.clone(), suggestions);
                    }
                }
            }
            (word.clone(), suggestions)
        })
        .collect()
}
fn run(text: &str, dictionary: Option<&str>) -> Result<Vec<(String, Vec<String>)>, String> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut command = Command::new("hunspell");
    command.args(["-a", "-i", "UTF-8"]);
    if let Some(dictionary) = dictionary {
        command.args(["-d", dictionary]);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            "Install Hunspell and a language dictionary to enable spelling suggestions.".to_owned()
        })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("Cannot open spelling checker input")?;
    let input = text
        .lines()
        .map(|line| format!("^{line}\n"))
        .collect::<String>();
    // Write concurrently: large documents can fill both child pipes.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    writer
        .join()
        .map_err(|_| "Spelling checker stopped")?
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Hunspell could not load its language dictionary.".into());
    }
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}
fn parse(output: &str) -> Vec<(String, Vec<String>)> {
    let mut issues = Vec::new();
    for line in output.lines() {
        let item = if let Some(rest) = line.strip_prefix("& ") {
            rest.split_once(": ").map(|(detail, suggestions)| {
                (
                    detail
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    suggestions.split(", ").take(6).map(str::to_owned).collect(),
                )
            })
        } else {
            line.strip_prefix("# ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(|word| (word.to_owned(), Vec::new()))
        };
        if let Some((word, suggestions)) = item
            && !word.is_empty()
            && !issues.iter().any(|(existing, _)| existing == &word)
        {
            issues.push((word, suggestions));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_misspellings_even_without_suggestions() {
        let issues =
            parse("@(#) Hunspell\n& teh 2 1: the, ten\n# qzxxxy 1\n*\n& teh 2 4: the, ten\n");
        assert_eq!(
            issues,
            vec![
                ("teh".into(), vec!["the".into(), "ten".into()]),
                ("qzxxxy".into(), vec![])
            ]
        );
    }
    #[test]
    fn discovers_supported_languages_and_falls_back_to_available_variants() {
        let available = "AVAILABLE DICTIONARIES:\n/usr/share/hunspell/en_GB\n/home/user/.local/share/hunspell/es_ES\n/usr/share/hunspell/ru_RU\n";
        assert_eq!(
            select_dictionaries(available),
            vec!["en_GB", "es_ES", "ru_RU"]
        );
        assert_eq!(
            select_dictionaries("/usr/share/hunspell/en_US\n"),
            vec!["en_US"]
        );
    }
    #[test]
    fn accepts_either_latin_language_and_keeps_suggestions_from_both() {
        let en = vec![
            ("hola".into(), vec!["hole".into()]),
            ("mañnaa".into(), vec!["manana".into()]),
        ];
        let es = vec![
            ("hello".into(), vec!["helio".into()]),
            ("mañnaa".into(), vec!["mañana".into()]),
        ];
        assert_eq!(
            merge_reports(vec![en, es]),
            vec![("mañnaa".into(), vec!["manana".into(), "mañana".into()])]
        );
    }
    #[test]
    #[ignore = "requires Hunspell with en_US, es_ES and ru_RU dictionaries installed"]
    fn multilingual_dictionary_integration() {
        let installed = dictionaries();
        assert!(installed.iter().any(|d| d == "en_US"));
        assert!(installed.iter().any(|d| d == "es_ES"));
        assert!(installed.iter().any(|d| d == "ru_RU"));
        let issues = check("hello hola mañana español привет слово speling mañnaa превет").unwrap();
        let issues: BTreeMap<_, _> = issues.into_iter().collect();
        assert_eq!(issues.len(), 3, "{issues:?}");
        assert!(issues["speling"].contains(&"spelling".into()));
        assert!(issues["mañnaa"].contains(&"mañana".into()));
        assert!(issues["превет"].contains(&"привет".into()));
    }
}
