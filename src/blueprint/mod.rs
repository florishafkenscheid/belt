use std::path::PathBuf;

use crate::{core::{BenchmarkError, Result}, util::blueprint_string::{parse_blueprint, save_blueprint_json, BlueprintString}};


#[derive(Debug, Clone)]
pub struct BlueprintConfig {
    pub string: Option<String>,
    pub file: PathBuf,
    pub output: PathBuf,
    pub recursive: bool,
}

pub async fn run(
    config: &BlueprintConfig
) -> Result<()> {
    tracing::debug!("Running blueprint generation");
    
    if config.string.is_some() {
        process_string(config)?;
    }
    let recursive = config.recursive;
    // Get the path provided by the blueprint config
    // and process that path.
    if config.file.is_file() {
        return process_file(&config, &config.file);
    } else if config.file.is_dir() {
        return process_directory(&config.file, &config, recursive);
    } else {
        tracing::error!("Provided path is neither a file nor a directory: {:?}", config.file);
        return Err(BenchmarkError::InvalidBlueprintPath { path: config.file.clone(), reason: "Provided path is neither a file nor a directory".into() });
    }
}

fn process_string(config: &BlueprintConfig) -> Result<()> {
    tracing::debug!("Using blueprint string: {:?}", config.string);

    let temp_file = config.file.with_file_name(".temp.txt");
    std::fs::write(&temp_file, config.string.as_ref().unwrap())?;
    tracing::debug!("Temporary blueprint file created at: {:?}", temp_file);

    let result = process_file(&config, &temp_file);

    std::fs::remove_file(&temp_file)?;
    tracing::debug!("Temporary blueprint file removed: {:?}", temp_file);
    return result;
}

fn create_save_file(
    blueprint: &BlueprintString,
    config: &BlueprintConfig,
) -> Result<()> {
    let output_path = &config.output;
    tracing::debug!("Creating save file at: {:?}", output_path);
    
    // Ensure the output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    save_blueprint_json(blueprint, output_path)
        .map_err(|e| BenchmarkError::BlueprintEncode { reason: e.to_string() })?;

    tracing::debug!("Save file created successfully at: {:?}", output_path);
    Ok(())
}


fn process_file(
    config: &BlueprintConfig,
    file_path: &PathBuf,
) -> Result<()> {
    tracing::debug!("Processing file: {:?}", file_path);
    
    if !file_path.exists() {
        tracing::error!("File does not exist: {:?}", file_path);
        return Err(BenchmarkError::InvalidBlueprintPath { path: file_path.clone(), reason: "File does not exist".into() });
    }

    let file_content = std::fs::read_to_string(file_path)
        .map_err(|e| BenchmarkError::InvalidBlueprintString { path: file_path.clone(), reason: e.to_string() })?;

    // crate::util::blueprint_string::from_str(&file_content)
    let blueprint = parse_blueprint(&file_content)
        .map_err(|e| BenchmarkError::BlueprintDecode { path: file_path.clone(), reason: e.to_string() })?;

    // Depending on Blueprint this might be huge
    tracing::debug!("Decoded file content: {:?}", blueprint);

    // We got the blueprint, as a json string.
    // From here we should type/parse it to a blueprint structure, then create a save file.
    create_save_file(&blueprint, &config)?;

    Ok(())
}

fn process_directory(
    path: &PathBuf,
    config: &BlueprintConfig,
    recursive: bool,
) -> Result<()> {
    tracing::debug!("Processing directory: {:?}", path);
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && recursive {
            tracing::debug!("Recursively processing directory: {:?}", path);
            process_directory(&path, &config, recursive)?;
        } else if path.is_file() {
            process_file(&config, &path)?;
        }
    }
    Ok(())
}