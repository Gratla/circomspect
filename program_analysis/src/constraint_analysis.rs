use log::{debug, trace};
use std::collections::{HashMap, HashSet};

use program_structure::cfg::Cfg;
use program_structure::intermediate_representation::variable_meta::VariableMeta;
use program_structure::intermediate_representation::AssignOp;
use program_structure::ir::variable_meta::VariableUse;
use program_structure::ir::{Statement, VariableName};

/// This analysis computes the transitive closure of the constraint relation.
/// (Note that the resulting relation will not be reflexive in general.)
#[derive(Clone, Default)]
pub struct ConstraintAnalysis {
    constraint_map: HashMap<VariableName, HashSet<VariableUse>>,
    definitions: HashMap<VariableName, VariableUse>,
}

impl ConstraintAnalysis {
    fn new() -> ConstraintAnalysis {
        ConstraintAnalysis::default()
    }

    /// Get the variable use corresponding to the definition of the variable.
    pub fn get_definition(&self, var: &VariableName) -> Option<VariableUse> {
        self.definitions.get(var).cloned()
    }

    pub fn definitions(&self) -> impl Iterator<Item = &VariableUse> {
        self.definitions.values()
    }

    /// Add the variable use corresponding to the definition of the variable.
    fn add_definition(&mut self, var: &VariableUse) {
        // TODO: Since we don't version components and signals, we may end up
        // overwriting component initializations here. For example, in the
        // following case the component initialization will be clobbered.
        //
        //   component c[2] = C();
        //   c[0].in[0] <== 0;
        //   c[1].in[1] <== 1;
        //
        // As long as the initialized component flows to a constraint it will
        // not be flagged during side-effect analysis.
        self.definitions.insert(var.name().clone(), var.clone());
    }

    /// Add a constraint from source to sink.
    fn add_constraint_step(&mut self, source: &VariableName, sink: &VariableUse) {
        let sinks = self.constraint_map.entry(source.clone()).or_default();
        sinks.insert(sink.clone());
    }

    /// Returns variables constrained in a single step by `source`.
    pub fn single_step_constraint(&self, source: &VariableName) -> HashSet<VariableUse> {
        self.constraint_map.get(source).cloned().unwrap_or_default()
    }

    /// Returns variables constrained in one or more steps by `source`.
    pub fn multi_step_constraint(&self, source: &VariableName) -> HashSet<VariableUse> {
        let mut result = HashSet::new();
        let mut update = self.single_step_constraint(source);
        while !update.is_subset(&result) {
            result.extend(update.iter().cloned());
            update = update
                .iter()
                .flat_map(|source| self.single_step_constraint(source.name()))
                .collect();
        }
        result
    }

    pub fn constrains_any(&self, source: &VariableName, sinks: &HashSet<VariableName>) -> bool {
        self.multi_step_constraint(source)
            .iter()
            .any(|sink| sinks.contains(sink.name()))
    }
}

pub fn run_constraint_analysis(cfg: &Cfg) -> ConstraintAnalysis {
    debug!("running taint analysis pass");
    let mut result = ConstraintAnalysis::new();

    use AssignOp::*;
    use Statement::*;
    for basic_block in cfg.iter() {
        for stmt in basic_block.iter() {
            trace!("visiting statement `{stmt:?}`");
            for var in stmt.variables_written() {
                result.add_definition(var);
            }
            if matches!(
                stmt,
                ConstraintEquality { .. }
                    | Substitution {
                        op: AssignConstraintSignal,
                        ..
                    }
            ) {
                for source in stmt.variables_used() {
                    for sink in stmt.variables_used() {
                        if source.name() != sink.name() {
                            trace!(
                                "adding constraint step with source `{:?}` and sink `{:?}`",
                                source.name(),
                                sink.name()
                            );
                            result.add_constraint_step(source.name(), sink);
                        }
                    }
                }
            }
        }
    }
    result
}
