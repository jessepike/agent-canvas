use std::fs;
use std::panic;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use similar::{ChangeTag, TextDiff};
use vellum_core::fs::save_round_trip;
use walkdir::WalkDir;

fn main() -> ExitCode {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let files = markdown_files(&corpus_dir);

    if files.is_empty() {
        eprintln!(
            "FAIL no Markdown corpus files found at {}",
            corpus_dir.display()
        );
        return ExitCode::FAILURE;
    }

    let mut failures = 0;

    for path in files {
        match check_file(&path) {
            Ok(()) => println!("PASS {}", path.display()),
            Err(error) => {
                failures += 1;
                eprintln!("FAIL {}\n{}", path.display(), error);
            }
        }
    }

    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        eprintln!("{failures} corpus file(s) failed");
        ExitCode::FAILURE
    }
}

fn markdown_files(corpus_dir: &Path) -> Vec<PathBuf> {
    let mut files = WalkDir::new(corpus_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect::<Vec<_>>();

    files.sort();
    files
}

fn check_file(path: &Path) -> Result<(), String> {
    let source_bytes = fs::read(path).map_err(|error| format!("could not read file: {error}"))?;
    let source = String::from_utf8(source_bytes.clone())
        .map_err(|error| format!("corpus file is not UTF-8: {error}"))?;

    let saved = catch_core_save(&source)
        .map_err(|panic| format!("core save panicked: {}", panic_message(panic)))?
        .map_err(|error| format!("core save failed: {error}"))?;
    let saved_bytes = saved.into_bytes();

    if source_bytes == saved_bytes {
        return Ok(());
    }

    let saved_text = String::from_utf8_lossy(&saved_bytes);
    let source_text = String::from_utf8_lossy(&source_bytes);
    let diff = TextDiff::from_lines(&source_text, &saved_text);
    let mut rendered = String::new();

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        rendered.push_str(sign);
        rendered.push_str(&change.to_string());
    }

    Err(rendered)
}

fn catch_core_save(
    source: &str,
) -> Result<Result<String, vellum_core::parse::ParseError>, Box<dyn std::any::Any + Send>> {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(|| save_round_trip(source));
    panic::set_hook(previous_hook);
    result
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_owned()
    }
}
