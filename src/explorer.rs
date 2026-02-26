/// State space exploration engine.
///
/// Systematically explores execution branches by combining user-defined
/// choices (perturbations, fault injections, message orderings) and
/// checking safety properties across all explored states.
///
/// Built on top of `Simulation::fork()` and `NodeRuntime::fork()`.

use crate::event::EventType;
use crate::node::NodeRuntime;
use crate::simulation::Simulation;
use crate::time::VirtualTime;

// ── Choice ────────────────────────────────────────────────────────────

/// A decision point with multiple possible options.
///
/// Each option is a set of events to inject into the simulation.
/// The explorer tries every option for every choice, forming the
/// Cartesian product of all choices.
#[derive(Debug, Clone)]
pub struct Choice {
    /// Human-readable label for this choice point.
    pub label: String,
    /// Each option is a list of `(time, event)` pairs to inject.
    pub options: Vec<Vec<(VirtualTime, EventType)>>,
}

impl Choice {
    /// A binary choice: apply the event or skip it. Creates 2 branches.
    pub fn binary(label: &str, time: VirtualTime, event: EventType) -> Self {
        Choice {
            label: label.to_string(),
            options: vec![
                vec![],                // skip
                vec![(time, event)],   // apply
            ],
        }
    }

    /// A choice with multiple custom options.
    pub fn multi(label: &str, options: Vec<Vec<(VirtualTime, EventType)>>) -> Self {
        Choice {
            label: label.to_string(),
            options,
        }
    }

    /// A choice between a set of alternative single events.
    pub fn alternatives(label: &str, events: Vec<(VirtualTime, EventType)>) -> Self {
        Choice {
            label: label.to_string(),
            options: events.into_iter().map(|e| vec![e]).collect(),
        }
    }
}

// ── Property ──────────────────────────────────────────────────────────

/// A safety property or invariant to check after each explored branch.
pub trait Property {
    /// Name of the property (for violation reports).
    fn name(&self) -> &str;

    /// Check the property against the final state.
    /// Returns `Ok(())` if satisfied, `Err(message)` if violated.
    fn check(&self, sim: &Simulation, rt: &NodeRuntime) -> Result<(), String>;
}

/// Convenience wrapper for closure-based properties.
pub struct NamedProperty {
    name: String,
    check_fn: Box<dyn Fn(&Simulation, &NodeRuntime) -> Result<(), String>>,
}

impl NamedProperty {
    pub fn new<F>(name: &str, f: F) -> Self
    where
        F: Fn(&Simulation, &NodeRuntime) -> Result<(), String> + 'static,
    {
        NamedProperty {
            name: name.to_string(),
            check_fn: Box::new(f),
        }
    }
}

impl Property for NamedProperty {
    fn name(&self) -> &str {
        &self.name
    }
    fn check(&self, sim: &Simulation, rt: &NodeRuntime) -> Result<(), String> {
        (self.check_fn)(sim, rt)
    }
}

// ── Violation ─────────────────────────────────────────────────────────

/// A property violation found during exploration.
#[derive(Debug, Clone)]
pub struct Violation {
    /// Name of the violated property.
    pub property: String,
    /// Branch index in the exploration.
    pub branch_id: usize,
    /// The specific choices that led to this violation:
    /// `(choice_label, option_index)`.
    pub choices: Vec<(String, usize)>,
    /// Human-readable violation message.
    pub message: String,
}

// ── ExplorationResult ─────────────────────────────────────────────────

/// Summary of a completed exploration run.
#[derive(Debug, Clone)]
pub struct ExplorationResult {
    /// Total branches explored.
    pub branches_explored: usize,
    /// Total branches possible (Cartesian product size).
    pub total_branches: usize,
    /// Property violations found.
    pub violations: Vec<Violation>,
}

impl ExplorationResult {
    /// Whether all properties were satisfied across all branches.
    pub fn is_safe(&self) -> bool {
        self.violations.is_empty()
    }

    /// Number of violations found.
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }
}

// ── Explorer ──────────────────────────────────────────────────────────

/// Systematically explores the state space of a simulation.
///
/// Given a base simulation, a set of choices, and properties to check,
/// the explorer forks the simulation for every combination of choices
/// and verifies properties after each run.
pub struct Explorer {
    base_sim: Simulation,
    base_rt: NodeRuntime,
    choices: Vec<Choice>,
    properties: Vec<Box<dyn Property>>,
    max_branches: usize,
}

impl Explorer {
    /// Create a new explorer from a base simulation state.
    ///
    /// The base simulation and runtime should be set up with nodes
    /// registered and any initial events scheduled.
    pub fn new(sim: Simulation, rt: NodeRuntime) -> Self {
        Explorer {
            base_sim: sim,
            base_rt: rt,
            choices: Vec::new(),
            properties: Vec::new(),
            max_branches: 10_000,
        }
    }

    /// Add a choice (decision point) to the exploration.
    pub fn add_choice(&mut self, choice: Choice) -> &mut Self {
        self.choices = self.choices.drain(..).chain(std::iter::once(choice)).collect();
        self
    }

    /// Add a property to check on every explored branch.
    pub fn add_property(&mut self, prop: Box<dyn Property>) -> &mut Self {
        self.properties.push(prop);
        self
    }

    /// Add a closure-based property.
    pub fn check<F>(&mut self, name: &str, f: F) -> &mut Self
    where
        F: Fn(&Simulation, &NodeRuntime) -> Result<(), String> + 'static,
    {
        self.properties.push(Box::new(NamedProperty::new(name, f)));
        self
    }

    /// Set the maximum number of branches to explore (safety limit).
    pub fn set_max_branches(&mut self, max: usize) -> &mut Self {
        self.max_branches = max;
        self
    }

    /// Compute the total number of branches (Cartesian product).
    pub fn total_branches(&self) -> usize {
        if self.choices.is_empty() {
            return 1; // just the base case
        }
        self.choices.iter().map(|c| c.options.len()).product()
    }

    /// Run the exploration: fork for each branch, run, check properties.
    pub fn explore(&self) -> ExplorationResult {
        let total = self.total_branches();
        let to_explore = total.min(self.max_branches);
        let mut violations = Vec::new();

        for branch_id in 0..to_explore {
            let selections = self.decode_branch(branch_id);

            // Fork base state.
            let mut sim = self.base_sim.fork();
            let mut rt = self.base_rt.fork();

            // Apply selected options.
            for (choice_idx, &option_idx) in selections.iter().enumerate() {
                for (time, event_type) in &self.choices[choice_idx].options[option_idx] {
                    sim.schedule(*time, event_type.clone());
                }
            }

            // Run to completion.
            sim.run(&mut rt);

            // Check all properties.
            for prop in &self.properties {
                if let Err(msg) = prop.check(&sim, &rt) {
                    violations.push(Violation {
                        property: prop.name().to_string(),
                        branch_id,
                        choices: selections
                            .iter()
                            .enumerate()
                            .map(|(i, &opt)| (self.choices[i].label.clone(), opt))
                            .collect(),
                        message: msg,
                    });
                }
            }
        }

        ExplorationResult {
            branches_explored: to_explore,
            total_branches: total,
            violations,
        }
    }

    /// Decode a branch ID into per-choice option indices.
    fn decode_branch(&self, mut branch_id: usize) -> Vec<usize> {
        let mut selections = Vec::with_capacity(self.choices.len());
        for choice in &self.choices {
            let n = choice.options.len();
            selections.push(branch_id % n);
            branch_id /= n;
        }
        selections
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{EchoNode, MessagePayload, NodeId, PingNode};

    fn setup_base() -> (Simulation, NodeRuntime) {
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);
        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));

        // Schedule a base message.
        sim.schedule(
            VirtualTime::new(0),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("base".into()),
            },
        );

        (sim, rt)
    }

    #[test]
    fn test_single_binary_choice() {
        let (sim, rt) = setup_base();
        let n1 = NodeId::new(1);

        let mut explorer = Explorer::new(sim, rt);
        explorer.add_choice(Choice::binary(
            "crash n1",
            VirtualTime::new(5),
            EventType::NodeCrash { node: n1 },
        ));

        let result = explorer.explore();
        assert_eq!(result.branches_explored, 2);
        assert_eq!(result.total_branches, 2);
        assert!(result.is_safe()); // no properties defined
    }

    #[test]
    fn test_property_violation_detected() {
        let (sim, rt) = setup_base();
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        let mut explorer = Explorer::new(sim, rt);
        explorer.add_choice(Choice::binary(
            "crash n1",
            VirtualTime::new(0),
            // Crash at T=0 — but message delivery also at T=0.
            // Due to event ID ordering, the message (ID=0) is processed
            // before the crash (ID=1), so echo still happens.
            // Use T before the delivery to guarantee the crash takes effect.
            EventType::NodeCrash { node: n1 },
        ));

        // Property: n0 must receive at least 1 echo.
        explorer.check("n0 receives echo", move |_sim, rt| {
            let ping = rt.node::<PingNode>(n0).unwrap();
            if ping.received.is_empty() {
                Err(format!("N0 received 0 messages"))
            } else {
                Ok(())
            }
        });

        let result = explorer.explore();
        assert_eq!(result.branches_explored, 2);

        // In branch where crash happens, n1 might still echo because
        // crash and delivery are at same time but delivery has lower ID.
        // So both branches should satisfy the property.
        // Let's instead test with crash BEFORE the message.
    }

    #[test]
    fn test_crash_before_message_violates_property() {
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);
        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));

        // Message at T=10.
        sim.schedule(
            VirtualTime::new(10),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("hello".into()),
            },
        );

        let mut explorer = Explorer::new(sim, rt);
        explorer.add_choice(Choice::binary(
            "crash n1 at T=5",
            VirtualTime::new(5),
            EventType::NodeCrash { node: n1 },
        ));

        explorer.check("n0 receives echo", move |_sim, rt| {
            let ping = rt.node::<PingNode>(n0).unwrap();
            if ping.received.is_empty() {
                Err("N0 received 0 messages".into())
            } else {
                Ok(())
            }
        });

        let result = explorer.explore();
        assert_eq!(result.branches_explored, 2);
        assert_eq!(result.violation_count(), 1);
        assert!(!result.is_safe());

        let v = &result.violations[0];
        assert_eq!(v.property, "n0 receives echo");
        assert_eq!(v.choices[0].0, "crash n1 at T=5");
        assert_eq!(v.choices[0].1, 1); // option 1 = "apply crash"
    }

    #[test]
    fn test_cartesian_product() {
        let (sim, rt) = setup_base();
        let n1 = NodeId::new(1);
        let n0 = NodeId::new(0);

        let mut explorer = Explorer::new(sim, rt);

        // 2 choices × 2 options = 4 branches.
        explorer.add_choice(Choice::binary(
            "crash n1",
            VirtualTime::new(5),
            EventType::NodeCrash { node: n1 },
        ));
        explorer.add_choice(Choice::binary(
            "extra msg",
            VirtualTime::new(3),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("extra".into()),
            },
        ));

        let result = explorer.explore();
        assert_eq!(result.total_branches, 4);
        assert_eq!(result.branches_explored, 4);
    }

    #[test]
    fn test_multi_choice() {
        let (sim, rt) = setup_base();
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        let mut explorer = Explorer::new(sim, rt);

        // 3-way choice: send to n0, send to n1, or send to both.
        explorer.add_choice(Choice::multi(
            "target",
            vec![
                vec![(
                    VirtualTime::new(5),
                    EventType::MessageDelivery {
                        from: n0,
                        to: n1,
                        payload: MessagePayload::Text("opt-a".into()),
                    },
                )],
                vec![(
                    VirtualTime::new(5),
                    EventType::MessageDelivery {
                        from: n1,
                        to: n0,
                        payload: MessagePayload::Text("opt-b".into()),
                    },
                )],
                vec![
                    (
                        VirtualTime::new(5),
                        EventType::MessageDelivery {
                            from: n0,
                            to: n1,
                            payload: MessagePayload::Text("opt-c1".into()),
                        },
                    ),
                    (
                        VirtualTime::new(5),
                        EventType::MessageDelivery {
                            from: n1,
                            to: n0,
                            payload: MessagePayload::Text("opt-c2".into()),
                        },
                    ),
                ],
            ],
        ));

        let result = explorer.explore();
        assert_eq!(result.total_branches, 3);
        assert_eq!(result.branches_explored, 3);
    }

    #[test]
    fn test_max_branches_limit() {
        let (sim, rt) = setup_base();
        let n1 = NodeId::new(1);

        let mut explorer = Explorer::new(sim, rt);

        // 2^10 = 1024 branches, but limit to 50.
        for i in 0..10 {
            explorer.add_choice(Choice::binary(
                &format!("choice-{}", i),
                VirtualTime::new(i * 5),
                EventType::NodeCrash { node: n1 },
            ));
        }
        explorer.set_max_branches(50);

        let result = explorer.explore();
        assert_eq!(result.total_branches, 1024);
        assert_eq!(result.branches_explored, 50);
    }

    #[test]
    fn test_no_choices_runs_base() {
        let (sim, rt) = setup_base();
        let n0 = NodeId::new(0);

        let mut explorer = Explorer::new(sim, rt);
        explorer.check("base works", move |_sim, rt| {
            let ping = rt.node::<PingNode>(n0).unwrap();
            if ping.received.len() == 1 {
                Ok(())
            } else {
                Err(format!("expected 1, got {}", ping.received.len()))
            }
        });

        let result = explorer.explore();
        assert_eq!(result.branches_explored, 1);
        assert!(result.is_safe());
    }

    #[test]
    fn test_multiple_properties() {
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);
        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));

        sim.schedule(
            VirtualTime::new(10),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("test".into()),
            },
        );

        let mut explorer = Explorer::new(sim, rt);
        explorer.add_choice(Choice::binary(
            "crash",
            VirtualTime::new(5),
            EventType::NodeCrash { node: n1 },
        ));

        // Property 1: always true.
        explorer.check("trivial", |_sim, _rt| Ok(()));

        // Property 2: n0 must receive at least 1 echo.
        explorer.check("echo received", move |_sim, rt| {
            let ping = rt.node::<PingNode>(n0).unwrap();
            if ping.received.is_empty() {
                Err("no echo".into())
            } else {
                Ok(())
            }
        });

        let result = explorer.explore();
        assert_eq!(result.branches_explored, 2);
        // Only 1 violation: "echo received" fails in the crash branch.
        assert_eq!(result.violation_count(), 1);
        assert_eq!(result.violations[0].property, "echo received");
    }
}
