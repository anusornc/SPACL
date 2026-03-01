use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::core::error::{OwlError, OwlResult};
use crate::core::ontology::Ontology;
use crate::parser::OntologyParser;
use crate::parser::ParserFactory;
use crate::serializer::BinaryOntologyFormat;
use crate::util::profiling::configure_iri_cache_for_large_ontology;

const TEXT_FALLBACK_MAX_BYTES: u64 = 32 * 1024 * 1024;
const LARGE_PROFILE_AUTO_THRESHOLD_BYTES: u64 = 4 * 1024 * 1024;
const RDF_XML_SNIFF_BYTES: usize = 16 * 1024;

fn env_truthy(key: &str) -> bool {
    match std::env::var(key) {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            !(value.is_empty() || value == "0" || value == "false" || value == "no")
        }
        Err(_) => false,
    }
}

fn env_is_set(key: &str) -> bool {
    match std::env::var(key) {
        Ok(value) => !value.trim().is_empty(),
        Err(_) => false,
    }
}

fn stage_timing_enabled() -> bool {
    env_truthy("OWL2_REASONER_STAGE_TIMING")
}

fn large_profile_auto_enabled() -> bool {
    if env_is_set("OWL2_REASONER_LARGE_PROFILE_AUTO") {
        return env_truthy("OWL2_REASONER_LARGE_PROFILE_AUTO");
    }
    true
}

fn large_profile_threshold_bytes() -> u64 {
    std::env::var("OWL2_REASONER_LARGE_PROFILE_THRESHOLD")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(LARGE_PROFILE_AUTO_THRESHOLD_BYTES)
}

fn path_looks_like_rdf_xml(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("rdf") | Some("rdfs") => true,
        Some("owl") | Some("xml") | Some("owx") => {
            let mut file = match std::fs::File::open(path) {
                Ok(f) => f,
                Err(_) => return false,
            };
            let mut buf = vec![0u8; RDF_XML_SNIFF_BYTES];
            let n = match file.read(&mut buf) {
                Ok(n) if n > 0 => n,
                _ => return false,
            };
            let sample = String::from_utf8_lossy(&buf[..n]);
            let trimmed = sample.trim_start();
            trimmed.starts_with("<?xml")
                || trimmed.starts_with("<rdf:RDF")
                || sample.contains("<rdf:RDF")
        }
        _ => false,
    }
}

fn should_apply_large_profile_auto(path: &Path) -> bool {
    if !large_profile_auto_enabled() {
        return false;
    }

    let file_size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(_) => return false,
    };
    if file_size < large_profile_threshold_bytes() {
        return false;
    }

    path_looks_like_rdf_xml(path)
}

fn source_path_from_bin(path: &Path) -> Option<PathBuf> {
    if path.extension().map(|e| e == "owlbin").unwrap_or(false) {
        Some(path.with_extension("owl"))
    } else {
        None
    }
}

fn stage_log(stage: &str, detail: &str) {
    if stage_timing_enabled() {
        eprintln!("[stage] {} {}", stage, detail);
    }
}

fn bin_path_for(path: &Path) -> PathBuf {
    if path.extension().map(|e| e == "owlbin").unwrap_or(false) {
        path.to_path_buf()
    } else {
        path.with_extension("owlbin")
    }
}

fn load_binary(path: &Path) -> OwlResult<Ontology> {
    let start = Instant::now();
    let file = std::fs::File::open(path)
        .map_err(|e| OwlError::StorageError(format!("Failed to open {}: {}", path.display(), e)))?;
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, file);
    let open_ms = start.elapsed().as_millis();
    stage_log(
        "binary_open_done",
        &format!("ms={} file={}", open_ms, path.display()),
    );

    let deser_start = Instant::now();
    let ontology = BinaryOntologyFormat::deserialize(&mut reader).map_err(|e| {
        OwlError::SerializationError(format!("Failed to deserialize {}: {}", path.display(), e))
    })?;
    let deser_ms = deser_start.elapsed().as_millis();
    stage_log(
        "binary_deserialize_done",
        &format!(
            "ms={} classes={} axioms={}",
            deser_ms,
            ontology.classes().len(),
            ontology.axioms().len()
        ),
    );
    Ok(ontology)
}

fn parse_text(path: &Path) -> OwlResult<Ontology> {
    let read_start = Instant::now();
    let content = std::fs::read_to_string(path)
        .map_err(|e| OwlError::StorageError(format!("Failed to read {}: {}", path.display(), e)))?;
    let read_ms = read_start.elapsed().as_millis();
    stage_log(
        "text_read_done",
        &format!(
            "ms={} bytes={} file={}",
            read_ms,
            content.len(),
            path.display()
        ),
    );

    let detect_start = Instant::now();
    let parser = ParserFactory::auto_detect(&content)
        .ok_or_else(|| OwlError::ParseError("Failed to detect ontology format".to_string()))?;
    let detect_ms = detect_start.elapsed().as_millis();
    stage_log("text_detect_done", &format!("ms={}", detect_ms));

    let parse_start = Instant::now();
    let ontology = parser
        .parse_str(&content)
        .map_err(|e| OwlError::ParseError(format!("{:?}", e)))?;
    let parse_ms = parse_start.elapsed().as_millis();
    stage_log(
        "text_parse_done",
        &format!(
            "ms={} classes={} axioms={}",
            parse_ms,
            ontology.classes().len(),
            ontology.axioms().len()
        ),
    );
    Ok(ontology)
}

fn detect_parser(path: &Path) -> Option<Box<dyn OntologyParser>> {
    let detect_start = Instant::now();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        if ext != "owl" {
            if let Some(parser) = ParserFactory::for_file_extension(ext) {
                stage_log(
                    "detect_parser_done",
                    &format!(
                        "ms={} source=extension ext={} found=1",
                        detect_start.elapsed().as_millis(),
                        ext
                    ),
                );
                return Some(parser);
            }
        }
    }

    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 16 * 1024];
    let n = file.read(&mut buf).ok()?;
    if n == 0 {
        return None;
    }
    let sample = String::from_utf8_lossy(&buf[..n]);
    let parser = ParserFactory::auto_detect(&sample).or_else(|| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(ParserFactory::for_file_extension)
    });
    stage_log(
        "detect_parser_done",
        &format!(
            "ms={} source=sample bytes={} found={}",
            detect_start.elapsed().as_millis(),
            n,
            if parser.is_some() { 1 } else { 0 }
        ),
    );
    parser
}

fn should_fallback_to_full_text_parse(path: &Path) -> bool {
    if env_truthy("OWL2_REASONER_DISABLE_PARSE_FALLBACK") {
        return false;
    }
    if env_truthy("OWL2_REASONER_ENABLE_PARSE_FALLBACK") {
        return true;
    }
    if env_truthy("OWL2_REASONER_LARGE_PARSE") {
        return false;
    }
    if should_apply_large_profile_auto(path) {
        return false;
    }

    match std::fs::metadata(path) {
        Ok(metadata) => metadata.len() <= TEXT_FALLBACK_MAX_BYTES,
        Err(_) => false,
    }
}

/// Load an ontology from a path with env-controlled binary caching behavior.
///
/// Environment variables:
/// - `OWL2_REASONER_FORCE_TEXT=1` -> ignore `.owlbin` and parse text.
/// - `OWL2_REASONER_BIN_ONLY=1`   -> require `.owlbin` (error if missing).
/// - `OWL2_REASONER_AUTO_CACHE=1` -> after parsing text, write `.owlbin`.
/// - `OWL2_REASONER_DISABLE_PARSE_FALLBACK=1` -> never retry with full-text parse.
/// - `OWL2_REASONER_ENABLE_PARSE_FALLBACK=1`  -> always retry with full-text parse.
/// - `OWL2_REASONER_LARGE_PROFILE_AUTO=0` -> disable auto text+structural profile for large RDF/XML.
/// - `OWL2_REASONER_LARGE_PROFILE_THRESHOLD=<bytes>` -> threshold for auto large-profile activation.
pub fn load_ontology_with_env(path: &Path) -> OwlResult<Ontology> {
    let load_start = Instant::now();
    stage_log("load_start", &format!("file={}", path.display()));

    if !path.exists() {
        return Err(OwlError::StorageError(format!(
            "File not found: {}",
            path.display()
        )));
    }

    let bin_only = env_truthy("OWL2_REASONER_BIN_ONLY");
    let mut force_text = env_truthy("OWL2_REASONER_FORCE_TEXT");
    let auto_cache = env_truthy("OWL2_REASONER_AUTO_CACHE");
    let force_text_explicit = env_is_set("OWL2_REASONER_FORCE_TEXT");
    let bin_only_explicit = env_is_set("OWL2_REASONER_BIN_ONLY");
    let input_is_bin = path.extension().map(|e| e == "owlbin").unwrap_or(false);

    let mut selected_path = path.to_path_buf();
    let mut auto_profile_redirect = false;
    let mut large_profile_auto = false;

    if input_is_bin
        && !bin_only
        && !force_text_explicit
        && !bin_only_explicit
        && large_profile_auto_enabled()
    {
        if let Some(text_path) = source_path_from_bin(path) {
            if text_path.exists() && should_apply_large_profile_auto(&text_path) {
                selected_path = text_path;
                force_text = true;
                large_profile_auto = true;
                auto_profile_redirect = true;
            }
        }
    }

    if !auto_profile_redirect {
        large_profile_auto = should_apply_large_profile_auto(&selected_path);
        if large_profile_auto && !force_text_explicit && !bin_only_explicit {
            force_text = true;
        }
    }

    let path_is_bin = selected_path
        .extension()
        .map(|e| e == "owlbin")
        .unwrap_or(false);
    let bin_path = bin_path_for(&selected_path);
    stage_log(
        "load_mode",
        &format!(
            "bin_only={} force_text={} auto_cache={} input_path_is_bin={} selected_path_is_bin={} selected_path={} bin_path={}",
            if bin_only { 1 } else { 0 },
            if force_text { 1 } else { 0 },
            if auto_cache { 1 } else { 0 },
            if input_is_bin { 1 } else { 0 },
            if path_is_bin { 1 } else { 0 },
            selected_path.display(),
            bin_path.display()
        ),
    );
    if large_profile_auto {
        stage_log(
            "load_mode_auto_profile",
            &format!(
                "enabled=1 threshold_bytes={} force_text_applied={} redirected_from_bin={}",
                large_profile_threshold_bytes(),
                if !force_text_explicit && !bin_only_explicit {
                    1
                } else {
                    0
                },
                if auto_profile_redirect { 1 } else { 0 }
            ),
        );
    }

    if bin_only {
        if !bin_path.exists() {
            return Err(OwlError::StorageError(format!(
                "Binary file required but not found: {}",
                bin_path.display()
            )));
        }
        return load_binary(&bin_path);
    }

    if path_is_bin {
        if force_text {
            return Err(OwlError::ConfigError {
                parameter: "OWL2_REASONER_FORCE_TEXT".to_string(),
                message: "Cannot force text parsing for .owlbin input".to_string(),
            });
        }
        return load_binary(&selected_path);
    }

    if !force_text && bin_path.exists() {
        let bin_load_start = Instant::now();
        match load_binary(&bin_path) {
            Ok(ontology) => {
                stage_log(
                    "load_done",
                    &format!(
                        "ms={} source=binary classes={} axioms={}",
                        load_start.elapsed().as_millis(),
                        ontology.classes().len(),
                        ontology.axioms().len()
                    ),
                );
                stage_log(
                    "binary_path_done",
                    &format!("ms={}", bin_load_start.elapsed().as_millis()),
                );
                return Ok(ontology);
            }
            Err(err) => {
                eprintln!(
                    "Failed to load {}: {:?}. Falling back to text parse.",
                    bin_path.display(),
                    err
                );
            }
        }
    }

    let file_size = std::fs::metadata(&selected_path).ok().map(|m| m.len());
    if let Some(file_size) = file_size {
        let estimated_classes = (file_size / 50) as usize;
        if estimated_classes > 10_000 {
            configure_iri_cache_for_large_ontology(estimated_classes);
        }
    }

    let ontology = if let Some(parser) = detect_parser(&selected_path) {
        let parse_file_start = Instant::now();
        match parser.parse_file(&selected_path) {
            Ok(ontology) => {
                stage_log(
                    "parse_file_done",
                    &format!(
                        "ms={} classes={} axioms={}",
                        parse_file_start.elapsed().as_millis(),
                        ontology.classes().len(),
                        ontology.axioms().len()
                    ),
                );
                ontology
            }
            Err(parse_err) => {
                if should_fallback_to_full_text_parse(&selected_path) {
                    eprintln!(
                        "Parser file-mode failed for {}: {:?}. Retrying with full-text parse.",
                        selected_path.display(),
                        parse_err
                    );
                    parse_text(&selected_path)?
                } else {
                    return Err(parse_err);
                }
            }
        }
    } else {
        parse_text(&selected_path)?
    };

    if auto_cache {
        let cache_start = Instant::now();
        if let Ok(file) = std::fs::File::create(&bin_path) {
            let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, file);
            if let Err(err) = BinaryOntologyFormat::serialize(&ontology, &mut writer) {
                eprintln!("Failed to write {}: {}", bin_path.display(), err);
            } else if let Err(err) = writer.flush() {
                eprintln!("Failed to flush {}: {}", bin_path.display(), err);
            } else {
                stage_log(
                    "auto_cache_done",
                    &format!(
                        "ms={} file={}",
                        cache_start.elapsed().as_millis(),
                        bin_path.display()
                    ),
                );
            }
        } else {
            eprintln!("Failed to create {}", bin_path.display());
        }
    }

    stage_log(
        "load_done",
        &format!(
            "ms={} source=text classes={} axioms={}",
            load_start.elapsed().as_millis(),
            ontology.classes().len(),
            ontology.axioms().len()
        ),
    );
    Ok(ontology)
}
