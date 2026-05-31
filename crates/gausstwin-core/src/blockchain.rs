//! Blockchain Integration Module
//!
//! Immutable audit trails and consensus mechanisms for distributed simulations

use crate::{error::Result, AgentId};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

/// Blockchain for simulation audit trails
#[derive(Debug)]
pub struct SimulationBlockchain {
    chain: Vec<Block>,
    pending_transactions: Vec<Transaction>,
    difficulty: usize,
    mining_reward: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: u64,
    pub timestamp: DateTime<Utc>,
    pub transactions: Vec<Transaction>,
    pub previous_hash: String,
    pub hash: String,
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub agent_id: AgentId,
    pub action: ActionType,
    pub timestamp: DateTime<Utc>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    AgentCreated,
    AgentMoved,
    AgentInteracted,
    StateChanged,
    Custom(String),
}

impl SimulationBlockchain {
    /// Create a new blockchain
    pub fn new(difficulty: usize) -> Self {
        let genesis_block = Block {
            index: 0,
            timestamp: Utc::now(),
            transactions: Vec::new(),
            previous_hash: "0".to_string(),
            hash: "genesis".to_string(),
            nonce: 0,
        };

        Self {
            chain: vec![genesis_block],
            pending_transactions: Vec::new(),
            difficulty,
            mining_reward: 1.0,
        }
    }

    /// Add a transaction to the pending pool
    pub fn add_transaction(&mut self, transaction: Transaction) {
        self.pending_transactions.push(transaction);
    }

    /// Mine a new block with pending transactions
    pub fn mine_block(&mut self) -> Result<Block> {
        let previous_block = self.chain.last().unwrap();
        let mut new_block = Block {
            index: previous_block.index + 1,
            timestamp: Utc::now(),
            transactions: self.pending_transactions.clone(),
            previous_hash: previous_block.hash.clone(),
            hash: String::new(),
            nonce: 0,
        };

        // Proof of work
        loop {
            let hash = self.calculate_hash(&new_block);
            if hash.starts_with(&"0".repeat(self.difficulty)) {
                new_block.hash = hash;
                break;
            }
            new_block.nonce += 1;
        }

        self.chain.push(new_block.clone());
        self.pending_transactions.clear();

        Ok(new_block)
    }

    /// Validate the entire blockchain
    pub fn is_chain_valid(&self) -> bool {
        for i in 1..self.chain.len() {
            let current_block = &self.chain[i];
            let previous_block = &self.chain[i - 1];

            // Check if current block hash is valid
            if current_block.hash != self.calculate_hash(current_block) {
                return false;
            }

            // Check if previous hash matches
            if current_block.previous_hash != previous_block.hash {
                return false;
            }
        }

        true
    }

    /// Get all transactions for a specific agent
    pub fn get_agent_history(&self, agent_id: AgentId) -> Vec<Transaction> {
        let mut history = Vec::new();

        for block in &self.chain {
            for transaction in &block.transactions {
                if transaction.agent_id == agent_id {
                    history.push(transaction.clone());
                }
            }
        }

        history
    }

    /// Get blockchain statistics
    pub fn get_stats(&self) -> BlockchainStats {
        let total_transactions = self
            .chain
            .iter()
            .map(|block| block.transactions.len())
            .sum();

        BlockchainStats {
            total_blocks: self.chain.len(),
            total_transactions,
            pending_transactions: self.pending_transactions.len(),
            difficulty: self.difficulty,
            chain_valid: self.is_chain_valid(),
        }
    }

    fn calculate_hash(&self, block: &Block) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&block.index.to_be_bytes());
        hasher.update(&block.timestamp.timestamp().to_be_bytes());
        hasher.update(block.previous_hash.as_bytes());
        hasher.update(&block.nonce.to_be_bytes());

        for transaction in &block.transactions {
            hasher.update(transaction.id.as_bytes());
            hasher.update(transaction.agent_id.to_string().as_bytes());
            hasher.update(&transaction.timestamp.timestamp().to_be_bytes());
            hasher.update(transaction.data.to_string().as_bytes());
        }

        STANDARD.encode(hasher.finalize().as_bytes())
    }
}

#[derive(Debug)]
pub struct BlockchainStats {
    pub total_blocks: usize,
    pub total_transactions: usize,
    pub pending_transactions: usize,
    pub difficulty: usize,
    pub chain_valid: bool,
}

/// Consensus mechanism for distributed simulations
#[derive(Debug)]
pub struct ConsensusEngine {
    nodes: HashMap<String, Node>,
    current_leader: Option<String>,
    consensus_type: ConsensusType,
    voting_power: HashMap<String, f64>,
}

#[derive(Debug)]
struct Node {
    id: String,
    stake: f64,
    reputation: f64,
    last_seen: u64,
}

#[derive(Debug)]
pub enum ConsensusType {
    ProofOfStake,
    ProofOfWork,
    DelegatedProofOfStake,
    Byzantine,
}

impl ConsensusEngine {
    /// Create a new consensus engine
    pub fn new(consensus_type: ConsensusType) -> Self {
        Self {
            nodes: HashMap::new(),
            current_leader: None,
            consensus_type,
            voting_power: HashMap::new(),
        }
    }

    /// Add a node to the consensus network
    pub fn add_node(&mut self, node_id: String, stake: f64) {
        let node = Node {
            id: node_id.clone(),
            stake,
            reputation: 1.0,
            last_seen: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        self.nodes.insert(node_id.clone(), node);
        self.voting_power.insert(node_id, stake);
    }

    /// Select a leader based on consensus algorithm
    pub fn select_leader(&mut self) -> Option<String> {
        match self.consensus_type {
            ConsensusType::ProofOfStake => {
                // Select leader based on stake probability
                let total_stake: f64 = self.voting_power.values().sum();
                if total_stake == 0.0 {
                    return None;
                }

                let random_value = rand::random::<f64>() * total_stake;
                let mut cumulative_stake = 0.0;

                for (node_id, stake) in &self.voting_power {
                    cumulative_stake += stake;
                    if random_value <= cumulative_stake {
                        self.current_leader = Some(node_id.clone());
                        return Some(node_id.clone());
                    }
                }

                None
            }
            _ => {
                // Simplified - would implement other consensus mechanisms
                let first_node = self.nodes.keys().next()?.clone();
                self.current_leader = Some(first_node.clone());
                Some(first_node)
            }
        }
    }

    /// Validate a proposed block through consensus
    pub fn validate_block(&self, _block: &Block, proposer: &str) -> bool {
        // Check if proposer is current leader
        if let Some(ref leader) = self.current_leader {
            leader == proposer
        } else {
            false
        }
    }

    /// Get current leader
    pub fn get_current_leader(&self) -> Option<&String> {
        self.current_leader.as_ref()
    }

    /// Get consensus statistics
    pub fn get_stats(&self) -> ConsensusStats {
        let total_stake = self.voting_power.values().sum();
        let active_nodes = self.nodes.len();

        ConsensusStats {
            total_nodes: active_nodes,
            total_stake,
            current_leader: self.current_leader.clone(),
            consensus_type: format!("{:?}", self.consensus_type),
        }
    }
}

#[derive(Debug)]
pub struct ConsensusStats {
    pub total_nodes: usize,
    pub total_stake: f64,
    pub current_leader: Option<String>,
    pub consensus_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockchain_creation() {
        let blockchain = SimulationBlockchain::new(2);
        assert_eq!(blockchain.chain.len(), 1); // Genesis block
        assert!(blockchain.is_chain_valid());
    }

    #[test]
    fn test_transaction_and_mining() {
        let mut blockchain = SimulationBlockchain::new(1);

        let transaction = Transaction {
            id: "tx1".to_string(),
            agent_id: AgentId::from_raw(1),
            action: ActionType::AgentCreated,
            timestamp: Utc::now(),
            data: serde_json::json!({"type": "agent_created"}),
        };

        blockchain.add_transaction(transaction);
        let block = blockchain.mine_block().unwrap();

        assert_eq!(block.transactions.len(), 1);
        assert_eq!(blockchain.chain.len(), 2);
        assert!(blockchain.is_chain_valid());
    }

    #[test]
    fn test_consensus_engine() {
        let mut consensus = ConsensusEngine::new(ConsensusType::ProofOfStake);

        consensus.add_node("node1".to_string(), 100.0);
        consensus.add_node("node2".to_string(), 200.0);
        consensus.add_node("node3".to_string(), 150.0);

        let leader = consensus.select_leader();
        assert!(leader.is_some());

        let stats = consensus.get_stats();
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.total_stake, 450.0);
    }
}
