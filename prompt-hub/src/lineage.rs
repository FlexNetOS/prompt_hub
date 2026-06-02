#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use std::collections::HashMap;
use tracing::{info, instrument, warn};

/// Prompt version lineage tracker.
///
/// Builds a version ancestry graph, tracks parent-child relationships,
/// and detects forks in the version history.
#[derive(Debug, Clone, Default)]
pub struct LineageTracker {
    /// version_id -> node
    nodes: HashMap<String, LineageNode>,
    /// root version IDs
    roots: Vec<String>,
}

/// A node in the version lineage graph.
#[derive(Debug, Clone)]
pub struct LineageNode {
    pub version_id: String,
    pub prompt_id: String,
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub created_at: String,
    pub author: String,
    pub is_fork: bool,
}

/// A fork in the version history.
#[derive(Debug, Clone)]
pub struct Fork {
    pub fork_point_version: String,
    pub branches: Vec<String>,
}

/// Version ancestry path from a node to the root.
#[derive(Debug, Clone)]
pub struct AncestryPath {
    pub version_id: String,
    pub path: Vec<String>,
    pub depth: usize,
}

/// Full lineage tree view.
#[derive(Debug, Clone)]
pub struct LineageTree {
    pub root: String,
    pub nodes: Vec<LineageNode>,
    pub depth: usize,
    pub fork_count: usize,
}

impl LineageTracker {
    /// Create a new lineage tracker.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            roots: Vec::new(),
        }
    }

    /// Register a new version in the lineage.
    #[instrument(skip(self), fields(version_id = %version_id, parent_id = ?parent_id))]
    pub fn register_version(
        &mut self,
        version_id: &str,
        prompt_id: &str,
        parent_id: Option<&str>,
        author: &str,
    ) -> Result<()> {
        if self.nodes.contains_key(version_id) {
            return Err(HubError::Conflict(format!(
                "Version '{}' already exists",
                version_id
            )));
        }

        let node = LineageNode {
            version_id: version_id.to_string(),
            prompt_id: prompt_id.to_string(),
            parent_id: parent_id.map(|s| s.to_string()),
            children: Vec::new(),
            created_at: "now".to_string(),
            author: author.to_string(),
            is_fork: false,
        };

        // Link to parent
        if let Some(pid) = parent_id {
            if let Some(parent) = self.nodes.get_mut(pid) {
                parent.children.push(version_id.to_string());
                // Check if this creates a fork (2+ children on same parent)
                if parent.children.len() > 1 {
                    parent.is_fork = true;
                    info!(
                        "Fork detected at version '{}': {} branches",
                        pid,
                        parent.children.len()
                    );
                }
            } else {
                return Err(HubError::NotFound(format!(
                    "Parent version '{}' not found",
                    pid
                )));
            }
        } else {
            self.roots.push(version_id.to_string());
            info!("Registered root version '{}'", version_id);
        }

        self.nodes.insert(version_id.to_string(), node);
        Ok(())
    }

    /// Get the ancestry path from a version back to root.
    #[instrument(skip(self), fields(version_id = %version_id))]
    pub fn get_ancestry(&self, version_id: &str) -> Result<AncestryPath> {
        let mut path = Vec::new();
        let mut current = version_id;

        while let Some(node) = self.nodes.get(current) {
            path.push(current.to_string());
            if let Some(ref parent) = node.parent_id {
                current = parent;
            } else {
                break;
            }
        }

        path.reverse();

        Ok(AncestryPath {
            version_id: version_id.to_string(),
            depth: path.len(),
            path,
        })
    }

    /// Detect all forks in the lineage.
    pub fn detect_forks(&self) -> Vec<Fork> {
        let mut forks = Vec::new();

        for (version_id, node) in &self.nodes {
            if node.is_fork && node.children.len() > 1 {
                forks.push(Fork {
                    fork_point_version: version_id.clone(),
                    branches: node.children.clone(),
                });
            }
        }

        forks
    }

    /// Get all children of a version (recursively).
    pub fn get_descendants(&self, version_id: &str) -> Vec<String> {
        let mut descendants = Vec::new();
        let mut queue = vec![version_id.to_string()];

        while let Some(current) = queue.pop() {
            if let Some(node) = self.nodes.get(&current) {
                for child in &node.children {
                    descendants.push(child.clone());
                    queue.push(child.clone());
                }
            }
        }

        descendants
    }

    /// Get siblings (other children of the same parent).
    pub fn get_siblings(&self, version_id: &str) -> Vec<String> {
        let node = match self.nodes.get(version_id) {
            Some(n) => n,
            None => return Vec::new(),
        };
        if let Some(ref parent_id) = node.parent_id {
            self.nodes
                .get(parent_id)
                .map(|parent| {
                    parent
                        .children
                        .iter()
                        .filter(|&c| c != version_id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Build a lineage tree view from a root.
    pub fn build_tree(&self, root_version: &str) -> Option<LineageTree> {
        let root_node = self.nodes.get(root_version)?;
        let mut all_nodes = Vec::new();
        let mut queue = vec![root_version.to_string()];
        let mut max_depth = 0;

        while let Some(current) = queue.pop() {
            if let Some(node) = self.nodes.get(&current) {
                max_depth += 1;
                all_nodes.push(node.clone());
                for child in &node.children {
                    queue.push(child.clone());
                }
            }
        }

        let forks = self.detect_forks();

        Some(LineageTree {
            root: root_version.to_string(),
            nodes: all_nodes,
            depth: max_depth,
            fork_count: forks.len(),
        })
    }

    /// Get all root versions.
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    /// Get node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get a specific node.
    pub fn get_node(&self, version_id: &str) -> Option<&LineageNode> {
        self.nodes.get(version_id)
    }

    /// Check if a version exists.
    pub fn has_version(&self, version_id: &str) -> bool {
        self.nodes.contains_key(version_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tracker() -> LineageTracker {
        LineageTracker::new()
    }

    #[test]
    fn test_register_root() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        assert_eq!(tracker.node_count(), 1);
        assert_eq!(tracker.roots().len(), 1);
    }

    #[test]
    fn test_register_child() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        tracker
            .register_version("v2", "prompt-1", Some("v1"), "bob")
            .unwrap();

        let ancestry = tracker.get_ancestry("v2").unwrap();
        assert_eq!(ancestry.path, vec!["v1", "v2"]);
        assert_eq!(ancestry.depth, 2);
    }

    #[test]
    fn test_detect_fork() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        tracker
            .register_version("v2", "prompt-1", Some("v1"), "bob")
            .unwrap();
        tracker
            .register_version("v3", "prompt-1", Some("v1"), "charlie")
            .unwrap();

        let forks = tracker.detect_forks();
        assert_eq!(forks.len(), 1);
        assert_eq!(forks[0].fork_point_version, "v1");
        assert_eq!(forks[0].branches.len(), 2);
    }

    #[test]
    fn test_get_descendants() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        tracker
            .register_version("v2", "prompt-1", Some("v1"), "bob")
            .unwrap();
        tracker
            .register_version("v3", "prompt-1", Some("v2"), "charlie")
            .unwrap();

        let descendants = tracker.get_descendants("v1");
        assert_eq!(descendants.len(), 2);
        assert!(descendants.contains(&"v2".to_string()));
        assert!(descendants.contains(&"v3".to_string()));
    }

    #[test]
    fn test_get_siblings() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        tracker
            .register_version("v2", "prompt-1", Some("v1"), "bob")
            .unwrap();
        tracker
            .register_version("v3", "prompt-1", Some("v1"), "charlie")
            .unwrap();

        let siblings = tracker.get_siblings("v2");
        assert_eq!(siblings, vec!["v3"]);
    }

    #[test]
    fn test_register_duplicate() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        let result = tracker.register_version("v1", "prompt-1", None, "bob");
        assert!(result.is_err());
    }

    #[test]
    fn test_register_missing_parent() {
        let mut tracker = make_tracker();
        let result = tracker.register_version("v2", "prompt-1", Some("v1"), "bob");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_tree() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        tracker
            .register_version("v2", "prompt-1", Some("v1"), "bob")
            .unwrap();

        let tree = tracker.build_tree("v1").unwrap();
        assert_eq!(tree.root, "v1");
        assert_eq!(tree.nodes.len(), 2);
    }

    #[test]
    fn test_has_version() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        assert!(tracker.has_version("v1"));
        assert!(!tracker.has_version("v99"));
    }

    #[test]
    fn test_get_node() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "prompt-1", None, "alice")
            .unwrap();
        let node = tracker.get_node("v1").unwrap();
        assert_eq!(node.author, "alice");
        assert_eq!(node.prompt_id, "prompt-1");
    }

    #[test]
    fn test_default() {
        let tracker: LineageTracker = Default::default();
        assert_eq!(tracker.node_count(), 0);
        assert!(tracker.roots().is_empty());
    }

    #[test]
    fn test_deep_ancestry() {
        let mut tracker = make_tracker();
        tracker
            .register_version("v1", "p1", None, "a")
            .unwrap();
        for i in 2..=5 {
            let prev = format!("v{}", i - 1);
            tracker
                .register_version(&format!("v{i}"), "p1", Some(&prev), "a")
                .unwrap();
        }
        let ancestry = tracker.get_ancestry("v5").unwrap();
        assert_eq!(ancestry.depth, 5);
        assert_eq!(ancestry.path[0], "v1");
        assert_eq!(ancestry.path[4], "v5");
    }

    #[test]
    fn test_multiple_roots() {
        let mut tracker = make_tracker();
        tracker
            .register_version("r1", "p1", None, "a")
            .unwrap();
        tracker
            .register_version("r2", "p2", None, "b")
            .unwrap();
        assert_eq!(tracker.roots().len(), 2);
        assert_eq!(tracker.node_count(), 2);
    }

    #[test]
    fn test_fork_struct() {
        let fork = Fork {
            fork_point_version: "v1".to_string(),
            branches: vec!["v2".to_string(), "v3".to_string()],
        };
        assert_eq!(fork.branches.len(), 2);
    }

    #[test]
    fn test_lineage_tree_clone() {
        let tree = LineageTree {
            root: "v1".to_string(),
            nodes: Vec::new(),
            depth: 1,
            fork_count: 0,
        };
        let cloned = tree.clone();
        assert_eq!(cloned.root, "v1");
    }
}
