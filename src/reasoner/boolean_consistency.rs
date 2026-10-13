//! Exact Boolean consistency certificates for the class-only OWL fragment.
//!
//! `check_boolean_consistency` returns `Some(true)` or `Some(false)` only
//! when every non-annotation logical axiom is in the supported Boolean class
//! fragment. Unsupported fragments return `None` before CNF encoding. Resource
//! exhaustion while inspecting or solving an exact Boolean candidate is an
//! error, never a satisfiable certificate.

use crate::core::error::{OwlError, OwlResult};
use crate::core::ontology::Ontology;
use crate::logic::axioms::{Axiom, ClassExpression};
use hashbrown::{HashMap, HashSet};
use std::collections::BTreeMap;

const OWL_THING_IRI: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_NOTHING_IRI: &str = "http://www.w3.org/2002/07/owl#Nothing";
const MAX_VARIABLES: usize = 2_000;
const MAX_CLAUSES: usize = 40_000;
const MAX_EXPR_DEPTH: usize = 256;
const MAX_PREFLIGHT_NODES: usize = 100_000;
const MAX_DPLL_BRANCH_DEPTH: usize = 256;
const MAX_DPLL_WORK_UNITS: usize = 2_000_000;

#[derive(Clone, Copy)]
enum Term {
    Const(bool),
    Lit(Lit),
}

#[derive(Clone, Copy)]
struct Lit {
    var: usize,
    positive: bool,
}

struct FormulaEncoder {
    named_vars: HashMap<String, usize>,
    variable_count: usize,
    clauses: Vec<Vec<Lit>>,
    complete: bool,
}

impl FormulaEncoder {
    fn new() -> Self {
        Self {
            named_vars: HashMap::new(),
            variable_count: 0,
            clauses: Vec::new(),
            complete: true,
        }
    }

    fn fork_with_tbox(&self) -> Self {
        Self {
            named_vars: self.named_vars.clone(),
            variable_count: self.variable_count,
            clauses: self.clauses.clone(),
            complete: self.complete,
        }
    }

    fn add_axiom(&mut self, axiom: &Axiom) -> OwlResult<()> {
        match axiom {
            Axiom::SubClassOf(axiom) => {
                let before = self.clauses.len();
                let Some(sub) = self.encode_expression(axiom.sub_class(), 0)? else {
                    self.complete = false;
                    self.clauses.truncate(before);
                    return Ok(());
                };
                let Some(sup) = self.encode_expression(axiom.super_class(), 0)? else {
                    self.complete = false;
                    self.clauses.truncate(before);
                    return Ok(());
                };
                self.add_clause([negate(sub), sup])
            }
            Axiom::EquivalentClasses(axiom) => {
                for (index, left) in axiom.classes().iter().enumerate() {
                    for right in axiom.classes().iter().skip(index + 1) {
                        let left = self.term_for_iri(left.as_str())?;
                        let right = self.term_for_iri(right.as_str())?;
                        self.add_clause([negate(left), right])?;
                        self.add_clause([negate(right), left])?;
                    }
                }
                Ok(())
            }
            Axiom::DisjointClasses(axiom) => {
                for (index, left) in axiom.classes().iter().enumerate() {
                    for right in axiom.classes().iter().skip(index + 1) {
                        let left = self.term_for_iri(left.as_str())?;
                        let right = self.term_for_iri(right.as_str())?;
                        self.add_clause([negate(left), negate(right)])?;
                    }
                }
                Ok(())
            }
            Axiom::AnnotationAssertion(_)
            | Axiom::SubAnnotationPropertyOf(_)
            | Axiom::AnnotationPropertyDomain(_)
            | Axiom::AnnotationPropertyRange(_) => Ok(()),
            _ => {
                self.complete = false;
                Ok(())
            }
        }
    }

    fn add_assertion(&mut self, expression: &ClassExpression) -> OwlResult<()> {
        let before = self.clauses.len();
        let Some(term) = self.encode_expression(expression, 0)? else {
            self.complete = false;
            self.clauses.truncate(before);
            return Ok(());
        };
        self.add_unit(term)
    }

    fn encode_expression(
        &mut self,
        expression: &ClassExpression,
        depth: usize,
    ) -> OwlResult<Option<Term>> {
        if depth > MAX_EXPR_DEPTH {
            return Err(resource_error(
                "boolean consistency expression depth",
                MAX_EXPR_DEPTH,
                "class expression nesting exceeds deterministic Boolean checker bound",
            ));
        }

        match expression {
            ClassExpression::Class(class) if class.iri().as_str() == OWL_THING_IRI => {
                Ok(Some(Term::Const(true)))
            }
            ClassExpression::Class(class) if class.iri().as_str() == OWL_NOTHING_IRI => {
                Ok(Some(Term::Const(false)))
            }
            ClassExpression::Class(class) => Ok(Some(self.term_for_iri(class.iri().as_str())?)),
            ClassExpression::ObjectComplementOf(inner) => {
                Ok(self.encode_expression(inner, depth + 1)?.map(negate))
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                if operands.is_empty() {
                    return Ok(Some(Term::Const(true)));
                }
                let mut terms = Vec::with_capacity(operands.len());
                for operand in operands {
                    let Some(term) = self.encode_expression(operand, depth + 1)? else {
                        return Ok(None);
                    };
                    terms.push(term);
                }
                self.encode_and(&terms).map(Some)
            }
            ClassExpression::ObjectUnionOf(operands) => {
                if operands.is_empty() {
                    return Ok(Some(Term::Const(false)));
                }
                let mut terms = Vec::with_capacity(operands.len());
                for operand in operands {
                    let Some(term) = self.encode_expression(operand, depth + 1)? else {
                        return Ok(None);
                    };
                    terms.push(term);
                }
                self.encode_or(&terms).map(Some)
            }
            _ => Ok(None),
        }
    }

    fn encode_and(&mut self, terms: &[Term]) -> OwlResult<Term> {
        if terms.iter().any(|term| matches!(term, Term::Const(false))) {
            return Ok(Term::Const(false));
        }
        let terms = terms
            .iter()
            .copied()
            .filter(|term| !matches!(term, Term::Const(true)))
            .collect::<Vec<_>>();
        if terms.is_empty() {
            return Ok(Term::Const(true));
        }
        if terms.len() == 1 {
            return Ok(terms[0]);
        }

        let output = self.fresh_aux()?;
        for term in &terms {
            self.add_clause([Term::Lit(negate_lit(output)), *term])?;
        }
        let mut reverse = Vec::with_capacity(terms.len() + 1);
        reverse.push(Term::Lit(output));
        reverse.extend(terms.iter().copied().map(negate));
        self.add_clause(reverse)?;
        Ok(Term::Lit(output))
    }

    fn encode_or(&mut self, terms: &[Term]) -> OwlResult<Term> {
        if terms.iter().any(|term| matches!(term, Term::Const(true))) {
            return Ok(Term::Const(true));
        }
        let terms = terms
            .iter()
            .copied()
            .filter(|term| !matches!(term, Term::Const(false)))
            .collect::<Vec<_>>();
        if terms.is_empty() {
            return Ok(Term::Const(false));
        }
        if terms.len() == 1 {
            return Ok(terms[0]);
        }

        let output = self.fresh_aux()?;
        for term in &terms {
            self.add_clause([Term::Lit(output), negate(*term)])?;
        }
        let mut reverse = Vec::with_capacity(terms.len() + 1);
        reverse.push(Term::Lit(negate_lit(output)));
        reverse.extend(terms.iter().copied());
        self.add_clause(reverse)?;
        Ok(Term::Lit(output))
    }

    fn add_unit(&mut self, term: Term) -> OwlResult<()> {
        self.add_clause([term])
    }

    fn add_clause<I>(&mut self, terms: I) -> OwlResult<()>
    where
        I: IntoIterator<Item = Term>,
    {
        let mut clause = Vec::new();
        let mut seen = HashSet::new();
        for term in terms {
            match term {
                Term::Const(true) => return Ok(()),
                Term::Const(false) => {}
                Term::Lit(lit) => {
                    let key = (lit.var, lit.positive);
                    if seen.contains(&(lit.var, !lit.positive)) {
                        return Ok(());
                    }
                    if seen.insert(key) {
                        clause.push(lit);
                    }
                }
            }
        }
        self.clauses.push(clause);
        self.check_clause_limit()
    }

    fn term_for_iri(&mut self, iri: &str) -> OwlResult<Term> {
        match iri {
            OWL_THING_IRI => Ok(Term::Const(true)),
            OWL_NOTHING_IRI => Ok(Term::Const(false)),
            _ => self.var_for_iri(iri).map(Term::Lit),
        }
    }

    fn var_for_iri(&mut self, iri: &str) -> OwlResult<Lit> {
        if let Some(var) = self.named_vars.get(iri) {
            return Ok(Lit {
                var: *var,
                positive: true,
            });
        }
        let var = self.variable_count;
        if var >= MAX_VARIABLES {
            return Err(resource_error(
                "boolean consistency variables",
                MAX_VARIABLES,
                "named and auxiliary Boolean variables exceed deterministic checker bound",
            ));
        }
        self.variable_count += 1;
        self.named_vars.insert(iri.to_string(), var);
        Ok(Lit {
            var,
            positive: true,
        })
    }

    fn fresh_aux(&mut self) -> OwlResult<Lit> {
        let var = self.variable_count;
        if var >= MAX_VARIABLES {
            return Err(resource_error(
                "boolean consistency variables",
                MAX_VARIABLES,
                "named and auxiliary Boolean variables exceed deterministic checker bound",
            ));
        }
        self.variable_count += 1;
        Ok(Lit {
            var,
            positive: true,
        })
    }

    fn check_clause_limit(&self) -> OwlResult<()> {
        if self.clauses.len() > MAX_CLAUSES {
            return Err(resource_error(
                "boolean consistency clauses",
                MAX_CLAUSES,
                "CNF clauses exceed deterministic checker bound",
            ));
        }
        Ok(())
    }

    fn is_satisfiable(&self) -> OwlResult<bool> {
        self.is_satisfiable_with_work_limit(MAX_DPLL_WORK_UNITS)
    }

    fn is_satisfiable_with_work_limit(&self, max_work_units: usize) -> OwlResult<bool> {
        let mut assignment = vec![None; self.variable_count];
        let mut budget = DpllBudget {
            work_remaining: max_work_units,
        };
        dpll(&self.clauses, &mut assignment, &mut budget, 0)
    }
}

pub(crate) fn check_boolean_consistency(ontology: &Ontology) -> OwlResult<Option<bool>> {
    if is_plain_named_subclass_taxonomy(ontology) {
        return Ok(None);
    }
    if !is_exact_boolean_fragment(ontology)? {
        return Ok(None);
    }

    let mut tbox = FormulaEncoder::new();
    let mut assertions_by_individual: BTreeMap<String, Vec<&ClassExpression>> = BTreeMap::new();

    for axiom in ontology.axioms() {
        match axiom.as_ref() {
            Axiom::ClassAssertion(assertion) => {
                assertions_by_individual
                    .entry(assertion.individual().as_str().to_string())
                    .or_default()
                    .push(assertion.class_expr());
            }
            other => tbox.add_axiom(other)?,
        }
    }

    if !tbox.complete {
        return Ok(None);
    }

    if !tbox.is_satisfiable()? {
        return Ok(Some(false));
    }

    for expressions in assertions_by_individual.values() {
        let mut individual = tbox.fork_with_tbox();
        for expression in expressions {
            individual.add_assertion(expression)?;
        }
        if !individual.complete {
            return Ok(None);
        }
        if !individual.is_satisfiable()? {
            return Ok(Some(false));
        }
    }

    Ok(Some(true))
}

fn is_exact_boolean_fragment(ontology: &Ontology) -> OwlResult<bool> {
    let mut expressions = Vec::new();

    for axiom in ontology.axioms() {
        match axiom.as_ref() {
            Axiom::SubClassOf(axiom) => {
                expressions.push(axiom.sub_class());
                expressions.push(axiom.super_class());
            }
            Axiom::EquivalentClasses(_) | Axiom::DisjointClasses(_) => {}
            Axiom::ClassAssertion(assertion) => {
                expressions.push(assertion.class_expr());
            }
            Axiom::AnnotationAssertion(_)
            | Axiom::SubAnnotationPropertyOf(_)
            | Axiom::AnnotationPropertyDomain(_)
            | Axiom::AnnotationPropertyRange(_) => {}
            _ => return Ok(false),
        }
    }

    for expression in &expressions {
        if !has_only_boolean_expression_shapes(expression)? {
            return Ok(false);
        }
    }

    for expression in expressions {
        validate_boolean_expression_depth(expression)?;
    }

    Ok(true)
}

fn has_only_boolean_expression_shapes(expression: &ClassExpression) -> OwlResult<bool> {
    let mut stack = vec![expression];
    let mut visited = 0;

    while let Some(current) = stack.pop() {
        visited += 1;
        if visited > MAX_PREFLIGHT_NODES {
            return Err(resource_error(
                "boolean consistency preflight nodes",
                MAX_PREFLIGHT_NODES,
                "class expression shape scan exceeds deterministic Boolean checker bound",
            ));
        }

        match current {
            ClassExpression::Class(_) => {}
            ClassExpression::ObjectComplementOf(inner) => stack.push(inner),
            ClassExpression::ObjectIntersectionOf(operands)
            | ClassExpression::ObjectUnionOf(operands) => {
                for operand in operands {
                    stack.push(operand);
                }
            }
            _ => return Ok(false),
        }
    }

    Ok(true)
}

fn validate_boolean_expression_depth(expression: &ClassExpression) -> OwlResult<()> {
    let mut stack = vec![(expression, 0)];
    let mut visited = 0;

    while let Some((current, depth)) = stack.pop() {
        visited += 1;
        if visited > MAX_PREFLIGHT_NODES {
            return Err(resource_error(
                "boolean consistency preflight nodes",
                MAX_PREFLIGHT_NODES,
                "class expression depth scan exceeds deterministic Boolean checker bound",
            ));
        }
        if depth > MAX_EXPR_DEPTH {
            return Err(resource_error(
                "boolean consistency expression depth",
                MAX_EXPR_DEPTH,
                "class expression nesting exceeds deterministic Boolean checker bound",
            ));
        }

        match current {
            ClassExpression::Class(_) => {}
            ClassExpression::ObjectComplementOf(inner) => stack.push((inner, depth + 1)),
            ClassExpression::ObjectIntersectionOf(operands)
            | ClassExpression::ObjectUnionOf(operands) => {
                for operand in operands {
                    stack.push((operand, depth + 1));
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn is_plain_named_subclass_taxonomy(ontology: &Ontology) -> bool {
    !ontology.axioms().is_empty()
        && ontology.axioms().iter().all(|axiom| match axiom.as_ref() {
            Axiom::SubClassOf(axiom) => {
                is_ordinary_named_class(axiom.sub_class())
                    && is_ordinary_named_class(axiom.super_class())
            }
            Axiom::AnnotationAssertion(_)
            | Axiom::SubAnnotationPropertyOf(_)
            | Axiom::AnnotationPropertyDomain(_)
            | Axiom::AnnotationPropertyRange(_) => true,
            _ => false,
        })
}

fn is_ordinary_named_class(expression: &ClassExpression) -> bool {
    matches!(
        expression,
        ClassExpression::Class(class)
            if class.iri().as_str() != OWL_THING_IRI && class.iri().as_str() != OWL_NOTHING_IRI
    )
}

struct DpllBudget {
    work_remaining: usize,
}

impl DpllBudget {
    fn consume(&mut self, units: usize) -> OwlResult<()> {
        if self.work_remaining < units {
            return Err(resource_error(
                "boolean consistency search work",
                MAX_DPLL_WORK_UNITS,
                "DPLL work exceeds deterministic checker bound",
            ));
        }
        self.work_remaining -= units;
        Ok(())
    }
}

fn dpll(
    clauses: &[Vec<Lit>],
    assignment: &mut [Option<bool>],
    budget: &mut DpllBudget,
    branch_depth: usize,
) -> OwlResult<bool> {
    if assignment.len() > MAX_VARIABLES {
        return Err(resource_error(
            "boolean consistency variables",
            MAX_VARIABLES,
            "DPLL assignment exceeds deterministic checker bound",
        ));
    }
    if branch_depth > MAX_DPLL_BRANCH_DEPTH {
        return Err(resource_error(
            "boolean consistency search depth",
            MAX_DPLL_BRANCH_DEPTH,
            "DPLL branching depth exceeds deterministic checker bound",
        ));
    }
    budget.consume(1)?;

    loop {
        budget.consume(1)?;
        let mut changed = false;
        let mut all_satisfied = true;

        for clause in clauses {
            budget.consume(1)?;
            let mut unassigned = None;
            let mut unassigned_count = 0;
            let mut satisfied = false;

            for lit in clause {
                budget.consume(1)?;
                match assignment[lit.var] {
                    Some(value) if value == lit.positive => {
                        satisfied = true;
                        break;
                    }
                    Some(_) => {}
                    None => {
                        unassigned = Some(*lit);
                        unassigned_count += 1;
                    }
                }
            }

            if satisfied {
                continue;
            }
            all_satisfied = false;
            if unassigned_count == 0 {
                return Ok(false);
            }
            if unassigned_count == 1 {
                if let Some(lit) = unassigned {
                    assignment[lit.var] = Some(lit.positive);
                    changed = true;
                }
            }
        }

        if all_satisfied {
            return Ok(true);
        }
        if !changed {
            break;
        }
    }

    let Some(var) = first_open_variable(clauses, assignment, budget)? else {
        return Ok(true);
    };

    for value in [false, true] {
        budget.consume(assignment.len().saturating_add(1))?;
        let mut branch = assignment.to_vec();
        branch[var] = Some(value);
        if dpll(clauses, &mut branch, budget, branch_depth + 1)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn first_open_variable(
    clauses: &[Vec<Lit>],
    assignment: &[Option<bool>],
    budget: &mut DpllBudget,
) -> OwlResult<Option<usize>> {
    for clause in clauses {
        budget.consume(1)?;
        for lit in clause {
            budget.consume(1)?;
            if assignment[lit.var].is_none() {
                return Ok(Some(lit.var));
            }
        }
    }
    Ok(None)
}

fn negate(term: Term) -> Term {
    match term {
        Term::Const(value) => Term::Const(!value),
        Term::Lit(lit) => Term::Lit(negate_lit(lit)),
    }
}

fn negate_lit(lit: Lit) -> Lit {
    Lit {
        var: lit.var,
        positive: !lit.positive,
    }
}

fn resource_error(resource_type: &str, limit: usize, message: &str) -> OwlError {
    OwlError::ResourceLimitExceeded {
        resource_type: resource_type.to_string(),
        limit,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::{Class, NamedIndividual, ObjectProperty};
    use crate::logic::axioms::{ClassAssertionAxiom, ObjectPropertyExpression, SubClassOfAxiom};

    fn class(iri: &str) -> Class {
        Class::new(format!("http://example.org/{iri}"))
    }

    fn expr(class: &Class) -> ClassExpression {
        ClassExpression::Class(class.clone())
    }

    fn not(expression: ClassExpression) -> ClassExpression {
        ClassExpression::ObjectComplementOf(Box::new(expression))
    }

    fn object_property(iri: &str) -> ObjectPropertyExpression {
        ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(format!(
            "http://example.org/{iri}"
        ))))
    }

    fn role_expression() -> ClassExpression {
        ClassExpression::ObjectSomeValuesFrom(
            Box::new(object_property("p")),
            Box::new(expr(&class("Filler"))),
        )
    }

    fn deep_boolean_expression(depth: usize) -> ClassExpression {
        let mut expression = expr(&class("Deep"));
        for _ in 0..depth {
            expression = not(expression);
        }
        expression
    }

    #[test]
    fn unsupported_mixed_contradiction_returns_none_before_solving() {
        let mut ontology = Ontology::new();
        let a = class("A");
        let filler = class("Filler");
        let x = NamedIndividual::new("http://example.org/x");

        ontology
            .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expr(&a)))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), not(expr(&a))))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                x.iri().clone(),
                ClassExpression::ObjectSomeValuesFrom(
                    Box::new(object_property("p")),
                    Box::new(expr(&filler)),
                ),
            ))
            .unwrap();

        assert!(matches!(check_boolean_consistency(&ontology), Ok(None)));
    }

    #[test]
    fn exact_deep_boolean_preflight_returns_resource_error() {
        let mut ontology = Ontology::new();
        let x = NamedIndividual::new("http://example.org/x");

        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                x.iri().clone(),
                deep_boolean_expression(257),
            ))
            .unwrap();

        let result = check_boolean_consistency(&ontology);
        assert!(matches!(
            result,
            Err(OwlError::ResourceLimitExceeded { resource_type, .. })
                if resource_type == "boolean consistency expression depth"
        ));
    }

    #[test]
    fn deep_boolean_before_later_unsupported_role_axiom_returns_none() {
        let mut ontology = Ontology::new();
        let x = NamedIndividual::new("http://example.org/x");

        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                x.iri().clone(),
                deep_boolean_expression(257),
            ))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), role_expression()))
            .unwrap();

        assert!(matches!(check_boolean_consistency(&ontology), Ok(None)));
    }

    #[test]
    fn deep_boolean_after_earlier_unsupported_role_axiom_returns_none() {
        let mut ontology = Ontology::new();
        let x = NamedIndividual::new("http://example.org/x");

        ontology
            .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), role_expression()))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                x.iri().clone(),
                deep_boolean_expression(257),
            ))
            .unwrap();

        assert!(matches!(check_boolean_consistency(&ontology), Ok(None)));
    }

    #[test]
    fn unsupported_sibling_below_deep_expression_returns_none() {
        let mut ontology = Ontology::new();
        let x = NamedIndividual::new("http://example.org/x");
        let expression = ClassExpression::ObjectUnionOf(
            vec![
                Box::new(deep_boolean_expression(257)),
                Box::new(role_expression()),
            ]
            .into(),
        );

        ontology
            .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expression))
            .unwrap();

        assert!(matches!(check_boolean_consistency(&ontology), Ok(None)));
    }

    #[test]
    fn dpll_work_budget_exhaustion_is_an_error() {
        let mut encoder = FormulaEncoder::new();
        let a = encoder.var_for_iri("http://example.org/A").unwrap();
        let b = encoder.var_for_iri("http://example.org/B").unwrap();
        encoder.add_clause([Term::Lit(a), Term::Lit(b)]).unwrap();

        let result = encoder.is_satisfiable_with_work_limit(1);
        assert!(matches!(
            result,
            Err(OwlError::ResourceLimitExceeded { resource_type, .. })
                if resource_type == "boolean consistency search work"
        ));
    }

    #[test]
    fn plain_taxonomy_uses_legacy_path() {
        let mut ontology = Ontology::new();
        let a = class("A");
        let b = class("B");

        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(expr(&a), expr(&b)))
            .unwrap();

        assert!(matches!(check_boolean_consistency(&ontology), Ok(None)));
    }
}
