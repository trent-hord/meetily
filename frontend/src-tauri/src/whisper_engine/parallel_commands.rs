use tauri::State;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::whisper_engine::{
    ParallelProcessor, ParallelConfig, SystemMonitor,
    AudioChunk, ProcessingStatus
};

// Global state for parallel processor
pub struct ParallelProcessorState {
    pub processor: Arc<RwLock<Option<ParallelProcessor>>>,
    pub system_monitor: Arc<SystemMonitor>,
}

impl ParallelProcessorState {
    pub fn new() -> Self {
        Self {
            processor: Arc::new(RwLock::new(None)),
            system_monitor: Arc::new(SystemMonitor::new()),
        }
    }
}

#[tauri::command]
pub async fn initialize_parallel_processor(
    state: State<'_, ParallelProcessorState>,
    max_workers: Option<usize>,
    memory_budget_mb: Option<u64>,
) -> Result<String, String> {
    let mut config = ParallelConfig::default();

    if let Some(workers) = max_workers {
        config.max_workers = std::cmp::min(workers, 4); // Safety limit
    }

    if let Some(memory) = memory_budget_mb {
        config.memory_budget_mb = memory;
    }

    // Calculate safe worker count based on system resources
    let safe_workers = state.system_monitor
        .calculate_safe_worker_count()
        .await
        .map_err(|e| format!("Failed to calculate safe worker count: {}", e))?;

    config.max_workers = std::cmp::min(config.max_workers, safe_workers);

    let (processor, _event_receiver) = ParallelProcessor::new(
        config.clone(),
        state.system_monitor.clone()
    ).map_err(|e| format!("Failed to create parallel processor: {}", e))?;

    *state.processor.write().await = Some(processor);

    Ok(format!("Parallel processor initialized with {} workers, {}MB memory per worker",
               config.max_workers, config.memory_budget_mb))
}

#[tauri::command]
pub async fn start_parallel_processing(
    state: State<'_, ParallelProcessorState>,
    audio_chunks: Vec<serde_json::Value>, // JSON representation of AudioChunk
    model_name: String,
) -> Result<String, String> {
    let chunks: Vec<AudioChunk> = audio_chunks
        .into_iter()
        .map(|v| serde_json::from_value(v))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to parse audio chunks: {}", e))?;

    let mut processor_guard = state.processor.write().await;
    let processor = processor_guard.as_mut()
        .ok_or_else(|| "Parallel processor not initialized".to_string())?;

    processor.start_processing(chunks.clone(), model_name.clone())
        .await
        .map_err(|e| format!("Failed to start parallel processing: {}", e))?;

    Ok(format!("Started parallel processing of {} chunks with model {}",
               chunks.len(), model_name))
}

#[tauri::command]
pub async fn pause_parallel_processing(
    state: State<'_, ParallelProcessorState>,
) -> Result<String, String> {
    let processor_guard = state.processor.read().await;
    let processor = processor_guard.as_ref()
        .ok_or_else(|| "Parallel processor not initialized".to_string())?;

    processor.pause_processing().await;
    Ok("Processing paused".to_string())
}

#[tauri::command]
pub async fn resume_parallel_processing(
    state: State<'_, ParallelProcessorState>,
) -> Result<String, String> {
    let processor_guard = state.processor.read().await;
    let processor = processor_guard.as_ref()
        .ok_or_else(|| "Parallel processor not initialized".to_string())?;

    processor.resume_processing().await;
    Ok("Processing resumed".to_string())
}

#[tauri::command]
pub async fn stop_parallel_processing(
    state: State<'_, ParallelProcessorState>,
) -> Result<String, String> {
    let mut processor_guard = state.processor.write().await;
    let processor = processor_guard.as_mut()
        .ok_or_else(|| "Parallel processor not initialized".to_string())?;

    processor.stop_processing().await;
    Ok("Processing stopped".to_string())
}

#[tauri::command]
pub async fn get_parallel_processing_status(
    state: State<'_, ParallelProcessorState>,
) -> Result<ProcessingStatus, String> {
    let processor_guard = state.processor.read().await;
    let processor = processor_guard.as_ref()
        .ok_or_else(|| "Parallel processor not initialized".to_string())?;

    let status = processor.get_processing_status().await;
    Ok(status)
}

#[tauri::command]
pub async fn get_system_resources(
    state: State<'_, ParallelProcessorState>,
) -> Result<serde_json::Value, String> {
    state.system_monitor.refresh_system_info()
        .await
        .map_err(|e| format!("Failed to refresh system info: {}", e))?;

    let resources = state.system_monitor.get_current_resources()
        .await
        .map_err(|e| format!("Failed to get system resources: {}", e))?;

    serde_json::to_value(resources)
        .map_err(|e| format!("Failed to serialize resources: {}", e))
}

#[tauri::command]
pub async fn check_resource_constraints(
    state: State<'_, ParallelProcessorState>,
) -> Result<serde_json::Value, String> {
    let status = state.system_monitor.check_resource_constraints()
        .await
        .map_err(|e| format!("Failed to check resource constraints: {}", e))?;

    serde_json::to_value(status)
        .map_err(|e| format!("Failed to serialize resource status: {}", e))
}

#[tauri::command]
pub async fn calculate_optimal_workers(
    state: State<'_, ParallelProcessorState>,
) -> Result<usize, String> {
    state.system_monitor.calculate_safe_worker_count()
        .await
        .map_err(|e| format!("Failed to calculate optimal workers: {}", e))
}

// Utility command to convert audio file to chunks for parallel processing
#[tauri::command]
pub async fn prepare_audio_chunks(
    audio_data: Vec<f32>,
    sample_rate: u32,
    chunk_duration_ms: Option<f64>, // Ignored now since we segment by speaker natively
) -> Result<Vec<AudioChunk>, String> {
    
    // 1. Download Pyannote models
    let embedding_model_path = match crate::pyannote::models::get_or_download_model(crate::pyannote::models::PyannoteModel::Embedding).await {
        Ok(p) => p,
        Err(e) => return Err(format!("Failed to get embedding model: {}", e)),
    };
    let segmentation_model_path = match crate::pyannote::models::get_or_download_model(crate::pyannote::models::PyannoteModel::Segmentation).await {
        Ok(p) => p,
        Err(e) => return Err(format!("Failed to get segmentation model: {}", e)),
    };

    // 2. Setup VAD Engine
    let mut vad_engine: Box<dyn crate::vad_engine::VadEngine + Send> = Box::new(crate::vad_engine::WebRtcVad::new());
    vad_engine.set_sensitivity(crate::vad_engine::VadSensitivity::Medium);
    let vad_engine_arc = std::sync::Arc::new(tokio::sync::Mutex::new(vad_engine));

    // 3. Setup Pyannote Extractors
    let embedding_extractor = match crate::pyannote::embedding::EmbeddingExtractor::new(embedding_model_path.to_str().unwrap()) {
        Ok(e) => std::sync::Arc::new(std::sync::Mutex::new(e)),
        Err(e) => return Err(format!("Failed to create embedding extractor: {}", e)),
    };

    let embedding_manager = crate::pyannote::identify::EmbeddingManager::new(10); 

    // 4. Extract Segments via Pyannote and VAD
    let segments_result = crate::segments::prepare_segments(
        &audio_data,
        vad_engine_arc,
        &segmentation_model_path,
        embedding_manager, 
        embedding_extractor,
        "cpu"
    ).await;

    let mut chunks = Vec::new();
    let mut chunk_id = 0;

    match segments_result {
        Ok((mut rx, _)) => {
            while let Some(segment) = rx.recv().await {
                // Segment is in 16khz right now according to segments.rs. But Pyannote segments
                // already output what the audio rate was. `AudioChunk` takes data.
                let chunk = AudioChunk {
                    id: chunk_id,
                    data: segment.samples.clone(),
                    sample_rate: segment.sample_rate,
                    start_time_ms: segment.start * 1000.0,
                    duration_ms: (segment.end - segment.start) * 1000.0,
                    speaker_id: Some(segment.speaker.clone()),
                };

                chunks.push(chunk);
                chunk_id += 1;
            }
        },
        Err(e) => {
            log::error!("Failed to run Pyannote segments: {:?}, falling back to blind generic chunks.", e);
            // Fallback to simple slicing if completely failed
            let duration_ms = chunk_duration_ms.unwrap_or(30000.0); // 30 seconds default
            let samples_per_chunk = ((sample_rate as f64 * duration_ms) / 1000.0) as usize;

            for (i, chunk_samples) in audio_data.chunks(samples_per_chunk).enumerate() {
                let start_time_ms = i as f64 * duration_ms;
                let actual_duration_ms = (chunk_samples.len() as f64 / sample_rate as f64) * 1000.0;

                let chunk = AudioChunk {
                    id: chunk_id,
                    data: chunk_samples.to_vec(),
                    sample_rate,
                    start_time_ms,
                    duration_ms: actual_duration_ms,
                    speaker_id: None,
                };
                chunks.push(chunk);
                chunk_id += 1;
            }
        }
    }

    if chunks.is_empty() {
        // Double fallback if Pyannote executed successfully but found ZERO speakers (utter silence or stripped)
        let duration_ms = chunk_duration_ms.unwrap_or(30000.0);
        let samples_per_chunk = ((sample_rate as f64 * duration_ms) / 1000.0) as usize;

        for (i, chunk_samples) in audio_data.chunks(samples_per_chunk).enumerate() {
            let start_time_ms = i as f64 * duration_ms;
            let actual_duration_ms = (chunk_samples.len() as f64 / sample_rate as f64) * 1000.0;

            let chunk = AudioChunk {
                id: chunk_id,
                data: chunk_samples.to_vec(),
                sample_rate,
                start_time_ms,
                duration_ms: actual_duration_ms,
                speaker_id: None,
            };
            chunks.push(chunk);
            chunk_id += 1;
        }
    }

    Ok(chunks)
}

// Test command for validating the parallel processing setup
#[tauri::command]
pub async fn test_parallel_processing_setup(
    state: State<'_, ParallelProcessorState>,
) -> Result<String, String> {
    let mut report = String::new();

    // Test system monitoring
    match state.system_monitor.get_current_resources().await {
        Ok(resources) => {
            report.push_str(&format!(
                "✅ System Resources: {:.1}% CPU, {:.1}% Memory, {} cores\n",
                resources.cpu_usage_percent,
                resources.memory_used_percent,
                resources.cpu_cores
            ));
        }
        Err(e) => {
            report.push_str(&format!("❌ System monitoring failed: {}\n", e));
        }
    }

    // Test resource constraints
    match state.system_monitor.check_resource_constraints().await {
        Ok(status) => {
            if status.can_proceed {
                report.push_str("✅ Resource constraints: All clear\n");
            } else {
                report.push_str(&format!(
                    "⚠️ Resource constraints: {}\n",
                    status.get_primary_constraint().unwrap_or("Unknown constraint".to_string())
                ));
            }
        }
        Err(e) => {
            report.push_str(&format!("❌ Resource constraint check failed: {}\n", e));
        }
    }

    // Test safe worker calculation
    match state.system_monitor.calculate_safe_worker_count().await {
        Ok(workers) => {
            report.push_str(&format!("✅ Safe worker count: {}\n", workers));
        }
        Err(e) => {
            report.push_str(&format!("❌ Worker calculation failed: {}\n", e));
        }
    }

    // Test parallel processor initialization
    let config = ParallelConfig::default();
    match ParallelProcessor::new(config, state.system_monitor.clone()) {
        Ok(_) => {
            report.push_str("✅ Parallel processor: Can be initialized\n");
        }
        Err(e) => {
            report.push_str(&format!("❌ Parallel processor initialization failed: {}\n", e));
        }
    }

    Ok(report)
}