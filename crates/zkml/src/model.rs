use ethereum_types::H256;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{Result, ZkMLError};

/// ML model formats supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelFormat {
    ONNX,
    PyTorch,
    TensorFlow,
    JAX,
    Candle,
}

/// ML model for zkML proving
#[derive(Debug, Clone)]
pub struct MLModel {
    id: String,
    format: ModelFormat,
    layers: Vec<Layer>,
    input_shape: Vec<usize>,
    output_shape: Vec<usize>,
    hash: H256,
}

impl MLModel {
    pub fn new(id: String, format: ModelFormat) -> Self {
        Self {
            id,
            format,
            layers: Vec::new(),
            input_shape: Vec::new(),
            output_shape: Vec::new(),
            hash: H256::zero(),
        }
    }
    
    pub fn id(&self) -> &str {
        &self.id
    }
    
    pub fn hash(&self) -> H256 {
        self.hash
    }
    
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
}

/// Model layer
#[derive(Debug, Clone)]
pub struct Layer {
    layer_type: String,
    weights: Vec<f32>,
    bias: Vec<f32>,
}

impl Layer {
    pub fn layer_type(&self) -> &str {
        &self.layer_type
    }
    
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }
    
    pub fn bias(&self) -> &[f32] {
        &self.bias
    }
    
    pub fn activation(&self) -> &str {
        "relu"
    }
    
    pub fn kernels(&self) -> &[f32] {
        &self.weights
    }
    
    pub fn stride(&self) -> usize {
        1
    }
    
    pub fn padding(&self) -> usize {
        0
    }
    
    pub fn num_heads(&self) -> usize {
        8
    }
    
    pub fn dim(&self) -> usize {
        512
    }
    
    pub fn qkv_weights(&self) -> &[f32] {
        &self.weights
    }
    
    pub fn gamma(&self) -> &[f32] {
        &self.weights
    }
    
    pub fn beta(&self) -> &[f32] {
        &self.bias
    }
    
    pub fn epsilon(&self) -> f32 {
        1e-5
    }
}

/// Model registry for managing models
pub struct ModelRegistry {
    models: std::collections::HashMap<String, Arc<MLModel>>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: std::collections::HashMap::new(),
        }
    }
    
    pub fn register(&mut self, model: MLModel) {
        self.models.insert(model.id.clone(), Arc::new(model));
    }
    
    pub fn get(&self, id: &str) -> Option<Arc<MLModel>> {
        self.models.get(id).cloned()
    }
}