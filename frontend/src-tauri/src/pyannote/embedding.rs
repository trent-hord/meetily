use crate::pyannote::session;
use anyhow::{Context, Result};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

#[derive(Debug)]
pub struct EmbeddingExtractor {
    session: Session,
}

impl EmbeddingExtractor {
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let session = session::create_session(model_path.as_ref())?;
        Ok(Self { session })
    }
    pub fn compute(&mut self, samples: &[f32]) -> Result<impl Iterator<Item = f32>> {
        let knf_features = knf_rs::compute_fbank(samples).map_err(anyhow::Error::msg)?;
        let shape = knf_features.shape().to_vec();
        let features = ndarray::Array2::from_shape_vec((shape[0], shape[1]), knf_features.into_raw_vec()).unwrap();
        let features = features.insert_axis(ndarray::Axis(0)); // Add batch dimension
        let ort_outs = self.session.run(ort::inputs!["feats" => TensorRef::from_array_view(features.view())?])?;
        let (_, data) = ort_outs
            .get("embs")
            .context("Output tensor not found")?
            .try_extract_tensor::<f32>()
            .context("Failed to extract tensor")?;

        // Collect the tensor data into a Vec to own it
        let embeddings: Vec<f32> = data.iter().copied().collect();

        // Return an iterator over the Vec
        Ok(embeddings.into_iter())
    }
}
