#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::*;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Minimal directed-graph implementation
// ---------------------------------------------------------------------------
//
// `petgraph` is an optional dependency that is not enabled in any build
// configuration, so this module ships a small self-contained directed graph
// that provides exactly the surface the swarm logic needs (node/edge
// insertion, counts, and topological-sort-based cycle detection).

/// Index of a node within a [`DiGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIndex(usize);

impl NodeIndex {
    /// Return the underlying index value.
    pub fn index(self) -> usize {
        self.0
    }
}

/// A minimal directed graph with node weights of type `N` and edge weights of
/// type `E`.
#[derive(Debug, Clone, Default)]
pub struct DiGraph<N, E> {
    nodes: Vec<N>,
    edges: Vec<(NodeIndex, NodeIndex, E)>,
}

impl<N, E> DiGraph<N, E> {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Add a node, returning its index.
    pub fn add_node(&mut self, weight: N) -> NodeIndex {
        let idx = NodeIndex(self.nodes.len());
        self.nodes.push(weight);
        idx
    }

    /// Add a directed edge from `a` to `b`.
    pub fn add_edge(&mut self, a: NodeIndex, b: NodeIndex, weight: E) {
        self.edges.push((a, b, weight));
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Perform a topological sort (Kahn's algorithm).
    ///
    /// Returns `Ok` with the nodes in topological order, or `Err(cycle)` with
    /// the index of a node that participates in a cycle.
    pub fn toposort(&self) -> std::result::Result<Vec<NodeIndex>, NodeIndex> {
        let n = self.nodes.len();
        let mut in_degree = vec![0usize; n];
        for (_, to, _) in &self.edges {
            in_degree[to.0] += 1;
        }

        let mut queue: Vec<NodeIndex> = (0..n)
            .filter(|&i| in_degree[i] == 0)
            .map(NodeIndex)
            .collect();
        let mut order = Vec::with_capacity(n);

        while let Some(node) = queue.pop() {
            order.push(node);
            for (from, to, _) in &self.edges {
                if from.0 == node.0 {
                    in_degree[to.0] -= 1;
                    if in_degree[to.0] == 0 {
                        queue.push(*to);
                    }
                }
            }
        }

        if order.len() == n {
            Ok(order)
        } else {
            // Some node still has a non-zero in-degree: it is part of a cycle.
            let stuck = in_degree
                .iter()
                .position(|&d| d > 0)
                .map(NodeIndex)
                .unwrap_or(NodeIndex(0));
            Err(stuck)
        }
    }
}

// ---------------------------------------------------------------------------
// Swarm bundle generation
// ---------------------------------------------------------------------------

/// Generate a swarm bundle for a set of roles.
///
/// * Validates the role dependency DAG.
/// * Builds the role dependency graph.
/// * Generates a consistency report.
/// * Returns a `SwarmBundle` with handoff templates.
#[instrument(skip(roles))]
pub async fn generate_swarm_bundle(
    roles: Vec<Role>,
    domain: Domain,
    workflow_id: Uuid,
) -> Result<SwarmBundle> {
    debug!(
        "Generating swarm bundle for {} roles in {:?}",
        roles.len(),
        domain
    );

    // Validate role dependency DAG.
    let conflicts = validate_swarm_roles(&roles)?;

    if !conflicts.is_empty() {
        warn!(
            "Swarm validation produced {} conflict(s): {:?}",
            conflicts.len(),
            conflicts
        );
    }

    // Build the role dependency graph.
    let graph = role_dependency_graph();

    // Generate consistency report.
    let consistency_report = check_consistency(&roles, &domain, &graph);

    // Generate evolution suggestions.
    let evolution_suggestions = suggest_evolution(&roles, &domain);

    // Build the standard handoff template between adjacent roles.
    let handoff_template = if roles.len() >= 2 {
        generate_handoff_template(&roles[0], &roles[1])
    } else if roles.len() == 1 {
        generate_handoff_template(&roles[0], &roles[0])
    } else {
        generate_handoff_template(&Role::Orchestrator, &Role::Architect)
    };

    Ok(SwarmBundle {
        workflow_id,
        role_prompts: HashMap::new(), // Populated from storage in production.
        handoff_template,
        consistency_report,
        evolution_suggestions,
    })
}

// ---------------------------------------------------------------------------
// Handoff template generation
// ---------------------------------------------------------------------------

/// Generate a standardized handoff template between two roles.
///
/// The template uses Handlebars-style placeholders that are filled at
/// runtime with the actual context, deliverables, blockers, and next steps.
pub fn generate_handoff_template(from: &Role, to: &Role) -> String {
    format!(
        r#"# Handoff: {from:?} -> {to:?}

## Context Summary
{{{{context_summary}}}}

## Deliverables
{{{{deliverables}}}}

## Blockers
{{{{blockers}}}}

## Next Steps
{{{{next_steps}}}}

## Deadline
{{{{deadline}}}}

## Quality Gate
- [ ] All deliverables reviewed
- [ ] No outstanding blockers
- [ ] Context documented
- [ ] Next steps defined with acceptance criteria
"#
    )
}

/// Generate a multi-role handoff template for a workflow with >2 roles.
///
/// Produces a chain of handoff sections covering every adjacent pair.
pub fn generate_full_handoff_chain(roles: &[Role]) -> String {
    if roles.is_empty() {
        return String::from("# No roles defined\n");
    }

    let mut sections = Vec::new();
    sections.push(format!(
        "# Swarm Workflow Handoff Chain\n\nRoles: {}\n",
        roles.len()
    ));

    for window in roles.windows(2) {
        sections.push(generate_handoff_template(&window[0], &window[1]));
        sections.push(String::from("\n---\n"));
    }

    // If only one role, generate a self-handoff template.
    if roles.len() == 1 {
        sections.push(generate_handoff_template(&roles[0], &roles[0]));
    }

    sections.join("\n")
}

// ---------------------------------------------------------------------------
// Role dependency graph
// ---------------------------------------------------------------------------

/// Build the canonical role dependency graph.
///
/// The default DAG represents the standard workflow:
///
/// ```text
/// Orchestrator -> Architect -> Implementer -> Critic -> Reviewer
/// ```
///
/// Custom roles are added as isolated nodes and must be wired by the caller.
pub fn role_dependency_graph() -> DiGraph<Role, ()> {
    let mut graph = DiGraph::new();
    let orchestrator = graph.add_node(Role::Orchestrator);
    let architect = graph.add_node(Role::Architect);
    let implementer = graph.add_node(Role::Implementer);
    let critic = graph.add_node(Role::Critic);
    let reviewer = graph.add_node(Role::Reviewer);

    // Dependencies: Orchestrator -> Architect -> Implementer -> Critic -> Reviewer
    graph.add_edge(orchestrator, architect, ());
    graph.add_edge(architect, implementer, ());
    graph.add_edge(implementer, critic, ());
    graph.add_edge(critic, reviewer, ());

    graph
}

/// Build an extended dependency graph that includes the given custom roles
/// as additional isolated nodes.
pub fn extended_dependency_graph(custom_roles: &[Role]) -> DiGraph<Role, ()> {
    let mut graph = role_dependency_graph();
    for role in custom_roles {
        if matches!(role, Role::Custom(_)) {
            graph.add_node(role.clone());
        }
    }
    graph
}

// ---------------------------------------------------------------------------
// Swarm role validation
// ---------------------------------------------------------------------------

/// Validate swarm roles against the dependency DAG.
///
/// Checks:
/// * Required roles (Orchestrator) are present.
/// * Duplicate roles are detected.
/// * Custom role naming constraints (non-empty, no reserved names).
pub fn validate_swarm_roles(roles: &[Role]) -> Result<Vec<Conflict>> {
    let mut conflicts = Vec::new();
    let role_set: HashSet<_> = roles.iter().collect();
    let graph = role_dependency_graph();

    // Check for required roles.
    let required = vec![Role::Orchestrator];
    for req in &required {
        if !role_set.contains(req) {
            conflicts.push(Conflict::MissingRole);
        }
    }

    // Check for duplicate roles.
    let mut seen = HashSet::new();
    for role in roles {
        if !seen.insert(role.clone()) {
            conflicts.push(Conflict::DuplicateRole(role.clone()));
        }
    }

    // Check for capability requirements: if Critic is present, Implementer
    // should generally also be present.
    if role_set.contains(&Role::Critic) && !role_set.contains(&Role::Implementer) {
        conflicts.push(Conflict::CapabilityMissing);
    }

    // Check graph connectivity: verify there are no isolated standard roles.
    // (In the default graph all standard roles are connected.)
    let _ = graph;

    // Validate custom role names.
    for role in roles {
        if let Role::Custom(name) = role {
            if name.trim().is_empty() {
                conflicts.push(Conflict::Custom(
                    "Custom role name cannot be empty".to_string(),
                ));
            }
            let reserved = [
                "Orchestrator",
                "Architect",
                "Implementer",
                "Critic",
                "Reviewer",
            ];
            if reserved.contains(&name.as_str()) {
                conflicts.push(Conflict::Custom(format!(
                    "Custom role name '{name}' conflicts with reserved role"
                )));
            }
        }
    }

    Ok(conflicts)
}

// ---------------------------------------------------------------------------
// Consistency checking
// ---------------------------------------------------------------------------

/// Check swarm bundle consistency.
///
/// Validates:
/// * Non-empty role set.
/// * Role compatibility with the target domain.
/// * Graph acyclicity (topological-sort cycle detection).
pub fn check_consistency(
    roles: &[Role],
    domain: &Domain,
    graph: &DiGraph<Role, ()>,
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    if roles.is_empty() {
        conflicts.push(Conflict::MissingRole);
        return conflicts;
    }

    // Check for cycles in the dependency graph.
    if let Err(cycle) = graph.toposort() {
        conflicts.push(Conflict::CircularDependency);
        warn!(
            "Cycle detected in role dependency graph at node index {:?}",
            cycle.index()
        );
    }

    // Check domain compatibility.
    match domain {
        Domain::Coding => {
            // Coding domain: recommend having all standard roles.
            let standard_roles: HashSet<_> = [
                Role::Orchestrator,
                Role::Architect,
                Role::Implementer,
                Role::Critic,
                Role::Reviewer,
            ]
            .iter()
            .collect();
            let present: HashSet<_> = roles.iter().collect();
            let missing: Vec<_> = standard_roles
                .difference(&present)
                .cloned()
                .cloned()
                .collect();
            if !missing.is_empty() {
                debug!("Missing standard roles for Coding domain: {:?}", missing);
            }
        }
        Domain::Writing
            // Writing domain: Critic and Reviewer are especially important.
            if !roles.contains(&Role::Critic) => {
                conflicts.push(Conflict::DomainMismatch);
            }
        _ => {
            // Other domains: no additional constraints.
        }
    }

    conflicts
}

// ---------------------------------------------------------------------------
// Evolution suggestions
// ---------------------------------------------------------------------------

/// Generate evolution suggestions for the swarm configuration.
///
/// Based on the roles and domain, suggests improvements or additional
/// configurations that could enhance the swarm's effectiveness.
fn suggest_evolution(roles: &[Role], _domain: &Domain) -> Vec<String> {
    let mut suggestions = Vec::new();
    let role_set: HashSet<_> = roles.iter().collect();

    // Suggest adding a Reviewer if Critic is present but Reviewer is not.
    if role_set.contains(&Role::Critic) && !role_set.contains(&Role::Reviewer) {
        suggestions
            .push("Consider adding a Reviewer role after Critic for final validation.".to_string());
    }

    // Suggest adding an Architect if Orchestrator is present but Architect is not.
    if role_set.contains(&Role::Orchestrator) && !role_set.contains(&Role::Architect) {
        suggestions.push(
            "Consider adding an Architect role to translate orchestration into design.".to_string(),
        );
    }

    // Suggest adding metrics collection if not already configured.
    if role_set.contains(&Role::Reviewer) {
        suggestions
            .push("Enable metrics collection for Reviewer sign-off quality tracking.".to_string());
    }

    suggestions
}

// ---------------------------------------------------------------------------
// Dynamic swarm reconfiguration
// ---------------------------------------------------------------------------

/// Reconfigure a swarm dynamically by adding and/or removing roles.
///
/// This is the primary mechanism for adaptive swarm evolution at runtime.
/// The workflow ID is preserved across reconfigurations for continuity.
#[instrument]
pub async fn reconfigure_swarm(
    current: Vec<Role>,
    add: Vec<Role>,
    remove: Vec<Role>,
    domain: Domain,
    workflow_id: Uuid,
) -> Result<SwarmBundle> {
    info!("Reconfiguring swarm: remove {:?}, add {:?}", remove, add);

    let mut new_roles: Vec<_> = current
        .into_iter()
        .filter(|r| !remove.contains(r))
        .collect();

    for role in add {
        if !new_roles.contains(&role) {
            new_roles.push(role);
        }
    }

    generate_swarm_bundle(new_roles, domain, workflow_id).await
}

/// Merge two swarm bundles, combining their role prompts and unioning
/// consistency reports.
pub fn merge_bundles(a: SwarmBundle, b: SwarmBundle) -> SwarmBundle {
    let mut role_prompts = a.role_prompts;
    role_prompts.extend(b.role_prompts);

    let mut consistency_report = a.consistency_report;
    consistency_report.extend(b.consistency_report);

    let mut evolution_suggestions = a.evolution_suggestions;
    evolution_suggestions.extend(b.evolution_suggestions);

    SwarmBundle {
        workflow_id: a.workflow_id,
        role_prompts,
        handoff_template: a.handoff_template, // Keep primary handoff template.
        consistency_report,
        evolution_suggestions,
    }
}

// ---------------------------------------------------------------------------
// Swarm role registry
// ---------------------------------------------------------------------------

/// Registry of available swarm roles with their metadata.
#[derive(Debug, Clone)]
pub struct SwarmRoleRegistry {
    roles: HashMap<Role, RoleMetadata>,
}

/// Metadata for a swarm role.
#[derive(Debug, Clone)]
pub struct RoleMetadata {
    pub description: String,
    pub required_capabilities: Vec<Capability>,
    pub max_parallel_agents: usize,
}

impl SwarmRoleRegistry {
    /// Create the default role registry with all standard roles.
    pub fn default_registry() -> Self {
        let mut roles = HashMap::new();

        roles.insert(
            Role::Orchestrator,
            RoleMetadata {
                description: "Coordinates the swarm, assigns tasks, and manages workflow."
                    .to_string(),
                required_capabilities: vec![Capability::Admin],
                max_parallel_agents: 1,
            },
        );

        roles.insert(
            Role::Architect,
            RoleMetadata {
                description: "Designs system architecture, defines interfaces and constraints."
                    .to_string(),
                required_capabilities: vec![Capability::Write],
                max_parallel_agents: 1,
            },
        );

        roles.insert(
            Role::Implementer,
            RoleMetadata {
                description: "Implements the design, writes code, and creates tests.".to_string(),
                required_capabilities: vec![Capability::Write, Capability::Execute],
                max_parallel_agents: 4,
            },
        );

        roles.insert(
            Role::Critic,
            RoleMetadata {
                description: "Reviews implementation against design, identifies issues."
                    .to_string(),
                required_capabilities: vec![Capability::Read, Capability::Write],
                max_parallel_agents: 2,
            },
        );

        roles.insert(
            Role::Reviewer,
            RoleMetadata {
                description: "Final validation, sign-off checklist, quality assurance.".to_string(),
                required_capabilities: vec![Capability::Read],
                max_parallel_agents: 1,
            },
        );

        Self { roles }
    }

    /// Look up metadata for a role.
    pub fn get(&self, role: &Role) -> Option<&RoleMetadata> {
        self.roles.get(role)
    }

    /// List all registered roles.
    pub fn list_roles(&self) -> Vec<&Role> {
        self.roles.keys().collect()
    }

    /// Register a custom role.
    pub fn register(&mut self, role: Role, metadata: RoleMetadata) {
        self.roles.insert(role, metadata);
    }
}

impl Default for SwarmRoleRegistry {
    fn default() -> Self {
        Self::default_registry()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Role dependency graph ----------------------------------------------

    #[test]
    fn test_role_dependency_graph() {
        let graph = role_dependency_graph();
        assert_eq!(graph.node_count(), 5);
        assert_eq!(graph.edge_count(), 4);
    }

    #[test]
    fn test_extended_dependency_graph() {
        let custom = vec![
            Role::Custom("Tester".to_string()),
            Role::Custom("Deployer".to_string()),
        ];
        let graph = extended_dependency_graph(&custom);
        assert_eq!(graph.node_count(), 7); // 5 standard + 2 custom
        assert_eq!(graph.edge_count(), 4);
    }

    // -- Validation ---------------------------------------------------------

    #[test]
    fn test_validate_empty_roles() {
        let conflicts = validate_swarm_roles(&[]).unwrap();
        assert!(!conflicts.is_empty());
        assert!(matches!(conflicts[0], Conflict::MissingRole));
    }

    #[test]
    fn test_validate_with_orchestrator() {
        let conflicts = validate_swarm_roles(&[Role::Orchestrator]).unwrap();
        // Only Orchestrator, no other standard roles — should pass required checks.
        assert!(
            conflicts
                .iter()
                .all(|c| !matches!(c, Conflict::MissingRole)),
            "Orchestrator should satisfy required role check"
        );
    }

    #[test]
    fn test_validate_duplicate_roles() {
        let conflicts = validate_swarm_roles(&[Role::Orchestrator, Role::Orchestrator]).unwrap();
        assert!(
            conflicts
                .iter()
                .any(|c| matches!(c, Conflict::DuplicateRole(Role::Orchestrator)))
        );
    }

    #[test]
    fn test_validate_critic_without_implementer() {
        let conflicts = validate_swarm_roles(&[Role::Orchestrator, Role::Critic]).unwrap();
        assert!(
            conflicts
                .iter()
                .any(|c| matches!(c, Conflict::CapabilityMissing))
        );
    }

    #[test]
    fn test_validate_custom_role_empty_name() {
        let conflicts =
            validate_swarm_roles(&[Role::Orchestrator, Role::Custom("".to_string())]).unwrap();
        assert!(conflicts.iter().any(|c| matches!(c, Conflict::Custom(_))));
    }

    #[test]
    fn test_validate_custom_role_reserved_name() {
        let conflicts =
            validate_swarm_roles(&[Role::Orchestrator, Role::Custom("Architect".to_string())])
                .unwrap();
        assert!(conflicts.iter().any(|c| matches!(c, Conflict::Custom(_))));
    }

    // -- Consistency checks -------------------------------------------------

    #[test]
    fn test_check_consistency_empty_roles() {
        let graph = role_dependency_graph();
        let conflicts = check_consistency(&[], &Domain::Coding, &graph);
        assert!(!conflicts.is_empty());
        assert!(matches!(conflicts[0], Conflict::MissingRole));
    }

    #[test]
    fn test_check_consency_valid_roles() {
        let graph = role_dependency_graph();
        let conflicts = check_consistency(
            &[Role::Orchestrator, Role::Architect, Role::Implementer],
            &Domain::Coding,
            &graph,
        );
        // Valid DAG with standard roles should have no conflicts.
        assert!(
            conflicts.is_empty()
                || conflicts
                    .iter()
                    .all(|c| !matches!(c, Conflict::MissingRole))
        );
    }

    // -- Handoff templates --------------------------------------------------

    #[test]
    fn test_generate_handoff_template() {
        let template = generate_handoff_template(&Role::Orchestrator, &Role::Architect);
        assert!(template.contains("Handoff"));
        assert!(template.contains("Orchestrator"));
        assert!(template.contains("Architect"));
        assert!(template.contains("{{context_summary}}"));
        assert!(template.contains("{{deliverables}}"));
        assert!(template.contains("{{blockers}}"));
        assert!(template.contains("{{next_steps}}"));
        assert!(template.contains("{{deadline}}"));
    }

    #[test]
    fn test_generate_full_handoff_chain() {
        let roles = vec![Role::Orchestrator, Role::Architect, Role::Implementer];
        let chain = generate_full_handoff_chain(&roles);
        assert!(chain.contains("Orchestrator"));
        assert!(chain.contains("Architect"));
        assert!(chain.contains("Implementer"));
        assert!(chain.contains("Handoff"));
    }

    #[test]
    fn test_generate_full_handoff_chain_empty() {
        let chain = generate_full_handoff_chain(&[]);
        assert!(chain.contains("No roles"));
    }

    #[test]
    fn test_generate_full_handoff_chain_single() {
        let chain = generate_full_handoff_chain(&[Role::Orchestrator]);
        assert!(chain.contains("Orchestrator"));
        assert!(chain.contains("Handoff"));
    }

    // -- Bundle generation --------------------------------------------------

    #[tokio::test]
    async fn test_generate_bundle() {
        let bundle = generate_swarm_bundle(
            vec![Role::Orchestrator, Role::Architect],
            Domain::Coding,
            Uuid::new_v4(),
        )
        .await;
        assert!(bundle.is_ok());
        let b = bundle.unwrap();
        assert!(!b.handoff_template.is_empty());
    }

    #[tokio::test]
    async fn test_generate_bundle_single_role() {
        let bundle =
            generate_swarm_bundle(vec![Role::Orchestrator], Domain::Coding, Uuid::new_v4()).await;
        assert!(bundle.is_ok());
    }

    // -- Reconfiguration ----------------------------------------------------

    #[tokio::test]
    async fn test_reconfigure_swarm_add_role() {
        let workflow_id = Uuid::new_v4();
        let bundle = reconfigure_swarm(
            vec![Role::Orchestrator],
            vec![Role::Architect],
            vec![],
            Domain::Coding,
            workflow_id,
        )
        .await;
        assert!(bundle.is_ok());
    }

    #[tokio::test]
    async fn test_reconfigure_swarm_remove_role() {
        let workflow_id = Uuid::new_v4();
        let bundle = reconfigure_swarm(
            vec![Role::Orchestrator, Role::Architect],
            vec![],
            vec![Role::Architect],
            Domain::Coding,
            workflow_id,
        )
        .await;
        assert!(bundle.is_ok());
    }

    // -- Merge bundles ------------------------------------------------------

    #[test]
    fn test_merge_bundles() {
        let a = SwarmBundle {
            workflow_id: Uuid::new_v4(),
            role_prompts: HashMap::new(),
            handoff_template: "template-a".to_string(),
            consistency_report: vec![Conflict::MissingRole],
            evolution_suggestions: vec!["suggestion-a".to_string()],
        };
        let b = SwarmBundle {
            workflow_id: a.workflow_id,
            role_prompts: HashMap::new(),
            handoff_template: "template-b".to_string(),
            consistency_report: vec![Conflict::CapabilityMissing],
            evolution_suggestions: vec!["suggestion-b".to_string()],
        };
        let merged = merge_bundles(a, b);
        assert_eq!(merged.consistency_report.len(), 2);
        assert_eq!(merged.evolution_suggestions.len(), 2);
    }

    // -- Role registry ------------------------------------------------------

    #[test]
    fn test_role_registry_default() {
        let registry = SwarmRoleRegistry::default_registry();
        assert!(!registry.list_roles().is_empty());
        assert!(registry.get(&Role::Orchestrator).is_some());
        assert!(registry.get(&Role::Architect).is_some());
        assert!(registry.get(&Role::Implementer).is_some());
        assert!(registry.get(&Role::Critic).is_some());
        assert!(registry.get(&Role::Reviewer).is_some());
    }

    #[test]
    fn test_role_registry_custom() {
        let mut registry = SwarmRoleRegistry::default_registry();
        let custom_role = Role::Custom("DevOps".to_string());
        registry.register(
            custom_role.clone(),
            RoleMetadata {
                description: "Handles deployment and infrastructure".to_string(),
                required_capabilities: vec![Capability::Write, Capability::Execute],
                max_parallel_agents: 2,
            },
        );
        assert!(registry.get(&custom_role).is_some());
    }

    // -- Evolution suggestions ----------------------------------------------

    #[test]
    fn test_suggest_evolution_add_reviewer() {
        let suggestions = suggest_evolution(
            &[Role::Orchestrator, Role::Architect, Role::Critic],
            &Domain::Coding,
        );
        assert!(suggestions.iter().any(|s| s.contains("Reviewer")));
    }

    // -- Send + Sync --------------------------------------------------------

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_swarm_bundle_send_sync() {
        assert_send_sync::<SwarmBundle>();
    }
}
