use ethereum_core::Transaction;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};

use crate::Result;

/// Transaction dependency graph
pub struct DependencyGraph {
    graph: DiGraph<usize, ()>,
    tx_to_node: HashMap<usize, NodeIndex>,
    dependencies: HashMap<usize, HashSet<usize>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            tx_to_node: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }
    
    /// Add a transaction to the graph
    pub fn add_transaction(&mut self, index: usize, _tx: Transaction) {
        let node = self.graph.add_node(index);
        self.tx_to_node.insert(index, node);
        self.dependencies.insert(index, HashSet::new());
    }
    
    /// Add a dependency edge
    pub fn add_dependency(&mut self, from: usize, to: usize) {
        if let (Some(&from_node), Some(&to_node)) = 
            (self.tx_to_node.get(&from), self.tx_to_node.get(&to)) {
            self.graph.add_edge(from_node, to_node, ());
            self.dependencies.get_mut(&to).unwrap().insert(from);
        }
    }
    
    /// Get dependencies for a transaction
    pub fn get_dependencies(&self, index: usize) -> Vec<usize> {
        self.dependencies
            .get(&index)
            .map(|deps| deps.iter().copied().collect())
            .unwrap_or_default()
    }
    
    /// Get dependents of a transaction
    pub fn get_dependents(&self, index: usize) -> Vec<usize> {
        let mut dependents = Vec::new();
        
        for (&tx_idx, deps) in &self.dependencies {
            if deps.contains(&index) {
                dependents.push(tx_idx);
            }
        }
        
        dependents
    }
    
    /// Check if graph has cycles
    pub fn has_cycles(&self) -> bool {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }
    
    /// Get topological ordering
    pub fn topological_order(&self) -> Result<Vec<usize>> {
        use petgraph::algo::toposort;
        
        toposort(&self.graph, None)
            .map(|nodes| nodes.iter().map(|&n| self.graph[n]).collect())
            .map_err(|_| crate::ParallelExecutionError::SchedulingError(
                "Cycle detected in dependency graph".to_string()
            ))
    }
    
    /// Convert to petgraph for advanced algorithms
    pub fn to_petgraph(&self) -> &DiGraph<usize, ()> {
        &self.graph
    }
    
    /// Find parallel groups (transactions with no dependencies on each other)
    pub fn find_parallel_groups(&self) -> Vec<Vec<usize>> {
        let mut groups = Vec::new();
        let mut processed = HashSet::new();
        let order = match self.topological_order() {
            Ok(o) => o,
            Err(_) => return groups,
        };
        
        for &tx_idx in &order {
            if processed.contains(&tx_idx) {
                continue;
            }
            
            let mut group = vec![tx_idx];
            processed.insert(tx_idx);
            
            // Find other transactions that can run in parallel
            for &other_idx in &order {
                if processed.contains(&other_idx) {
                    continue;
                }
                
                // Check if they have dependencies on each other
                if !self.has_path(tx_idx, other_idx) && !self.has_path(other_idx, tx_idx) {
                    group.push(other_idx);
                    processed.insert(other_idx);
                }
            }
            
            groups.push(group);
        }
        
        groups
    }
    
    /// Check if there's a path from one transaction to another
    fn has_path(&self, from: usize, to: usize) -> bool {
        use petgraph::algo::has_path_connecting;
        
        if let (Some(&from_node), Some(&to_node)) = 
            (self.tx_to_node.get(&from), self.tx_to_node.get(&to)) {
            has_path_connecting(&self.graph, from_node, to_node, None)
        } else {
            false
        }
    }
}

/// Transaction node in the dependency graph
#[derive(Debug, Clone)]
pub struct TransactionNode {
    pub index: usize,
    pub transaction: Transaction,
    pub dependencies: Vec<usize>,
    pub dependents: Vec<usize>,
}