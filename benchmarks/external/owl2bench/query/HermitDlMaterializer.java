import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.PrintStream;
import java.security.MessageDigest;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.Set;
import java.util.TreeSet;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import org.eclipse.rdf4j.model.IRI;
import org.eclipse.rdf4j.model.Literal;
import org.eclipse.rdf4j.model.Model;
import org.eclipse.rdf4j.model.BNode;
import org.eclipse.rdf4j.model.Resource;
import org.eclipse.rdf4j.model.Value;
import org.eclipse.rdf4j.model.impl.LinkedHashModel;
import org.eclipse.rdf4j.model.impl.SimpleValueFactory;
import org.eclipse.rdf4j.model.vocabulary.OWL;
import org.eclipse.rdf4j.model.vocabulary.RDF;
import org.eclipse.rdf4j.model.vocabulary.RDFS;
import org.eclipse.rdf4j.rio.RDFFormat;
import org.eclipse.rdf4j.rio.RDFParser;
import org.eclipse.rdf4j.rio.RDFWriter;
import org.eclipse.rdf4j.rio.helpers.StatementCollector;
import org.eclipse.rdf4j.rio.nquads.NQuadsParser;
import org.eclipse.rdf4j.rio.nquads.NQuadsWriter;
import org.eclipse.rdf4j.rio.rdfxml.RDFXMLParser;
import org.semanticweb.HermiT.Configuration;
import org.semanticweb.HermiT.Reasoner;
import org.semanticweb.owlapi.apibinding.OWLManager;
import org.semanticweb.owlapi.model.OWLClass;
import org.semanticweb.owlapi.model.OWLDataFactory;
import org.semanticweb.owlapi.model.OWLDataProperty;
import org.semanticweb.owlapi.model.OWLDataPropertyExpression;
import org.semanticweb.owlapi.model.OWLAnonymousIndividual;
import org.semanticweb.owlapi.model.OWLNamedIndividual;
import org.semanticweb.owlapi.model.OWLObjectProperty;
import org.semanticweb.owlapi.model.OWLObjectPropertyExpression;
import org.semanticweb.owlapi.model.OWLClassExpression;
import org.semanticweb.owlapi.model.OWLDataRange;
import org.semanticweb.owlapi.model.OWLDataSomeValuesFrom;
import org.semanticweb.owlapi.model.OWLDataMinCardinality;
import org.semanticweb.owlapi.model.OWLDataExactCardinality;
import org.semanticweb.owlapi.model.OWLObjectExactCardinality;
import org.semanticweb.owlapi.model.OWLObjectMinCardinality;
import org.semanticweb.owlapi.model.OWLObjectSomeValuesFrom;
import org.semanticweb.owlapi.model.OWLAxiom;
import org.semanticweb.owlapi.model.OWLEquivalentClassesAxiom;
import org.semanticweb.owlapi.model.OWLSubPropertyChainOfAxiom;
import org.semanticweb.owlapi.model.OWLSubClassOfAxiom;
import org.semanticweb.owlapi.model.OWLOntology;
import org.semanticweb.owlapi.model.OWLOntologyManager;
import org.semanticweb.owlapi.reasoner.InferenceType;
import org.semanticweb.owlapi.reasoner.NodeSet;

/**
 * HermiT-backed finite DL materializer used by the profile-specific adapter.
 *
 * <p>The source RDF graph is the immutable base of the candidate output. The
 * reasoner contributes only named-term entailments selected by the official
 * query vocabulary and profile signature. HermiT-generated anonymous tableau
 * witnesses are never copied into the output.</p>
 */
public final class HermitDlMaterializer {
    private static final String OWL2BENCH_NAMESPACE =
            "https://kracr.iiitd.edu.in/OWL2Bench#";
    private static final String MATERIALIZATION_MODE = "finite_query_complete";
    private static final Pattern PREFIX_DECLARATION = Pattern.compile(
            "(?im)^\\s*PREFIX\\s+([A-Za-z][A-Za-z0-9_-]*)?\\s*:\\s*<([^>]+)>");
    private static final Pattern PREFIXED_NAME = Pattern.compile(
            "(?<![A-Za-z0-9_])([A-Za-z][A-Za-z0-9_-]*)?:([A-Za-z_][A-Za-z0-9_.-]*)");
    private static final Pattern FULL_IRI = Pattern.compile("<([^>]+)>");
    private static final Set<String> CLASS_HIERARCHY_PREDICATES = new HashSet<>(
            Arrays.asList(RDFS.SUBCLASSOF.stringValue(), OWL.EQUIVALENTCLASS.stringValue()));
    private static final Set<String> PROPERTY_HIERARCHY_PREDICATES = new HashSet<>(
            Arrays.asList(
                    RDFS.SUBPROPERTYOF.stringValue(),
                    OWL.EQUIVALENTPROPERTY.stringValue(),
                    OWL.INVERSEOF.stringValue(),
                    RDFS.DOMAIN.stringValue(),
                    RDFS.RANGE.stringValue()));

    private static final class NotAvailableException extends Exception {
        private static final long serialVersionUID = 1L;

        NotAvailableException(String message) {
            super(message);
        }
    }

    private static final class ObjectWitnessRequirement {
        private final OWLObjectPropertyExpression property;
        private final OWLClassExpression filler;
        private final int minimumCardinality;

        private ObjectWitnessRequirement(
                OWLObjectPropertyExpression property,
                OWLClassExpression filler,
                int minimumCardinality) {
            this.property = property;
            this.filler = filler;
            this.minimumCardinality = minimumCardinality;
        }
    }

    private static final class DataWitnessRequirement {
        private final OWLDataPropertyExpression property;
        private final OWLDataRange filler;
        private final int minimumCardinality;

        private DataWitnessRequirement(
                OWLDataPropertyExpression property,
                OWLDataRange filler,
                int minimumCardinality) {
            this.property = property;
            this.filler = filler;
            this.minimumCardinality = minimumCardinality;
        }
    }

    private HermitDlMaterializer() {}

    public static void main(String[] arguments) throws Exception {
        try {
            if (arguments.length < 2) {
                usage();
            }
            String command = arguments[0];
            if ("source".equals(command) && arguments.length == 2) {
                writeNQuads(parseRdfXml(Paths.get(arguments[1])), System.out);
                return;
            }
            if ("version".equals(command) && arguments.length == 3) {
                printVersion(Paths.get(arguments[1]), Paths.get(arguments[2]));
                return;
            }
            if ("export".equals(command) && arguments.length == 5) {
                export(
                        Paths.get(arguments[1]),
                        Paths.get(arguments[2]),
                        Paths.get(arguments[3]),
                        Paths.get(arguments[4]),
                        System.out);
                return;
            }
            usage();
        } catch (NotAvailableException error) {
            System.err.println("[hermit-dl-materializer][not_available] " + error.getMessage());
            System.exit(125);
        }
    }

    private static void usage() {
        System.err.println(
                "usage: source <ontology.rdf> | version <ontology.rdf> <config.properties> "
                        + "| export <ontology.rdf> <source.nq> <query-dir> <config.properties>");
        System.exit(2);
    }

    private static Properties readConfiguration(Path path) throws Exception {
        Properties configuration = new Properties();
        try (InputStream input = new FileInputStream(path.toFile())) {
            configuration.load(input);
        }
        if (!configuration.containsKey("materialization.mode")
                || configuration.getProperty("materialization.mode", "").trim().isEmpty()
                || !configuration.containsKey("engine.id")
                || configuration.getProperty("engine.id", "").trim().isEmpty()
                || !configuration.containsKey("hermit.version")
                || configuration.getProperty("hermit.version", "").trim().isEmpty()) {
            throw new NotAvailableException("HermiT configuration pin is missing");
        }
        if (!MATERIALIZATION_MODE.equals(
                configuration.getProperty("materialization.mode"))) {
            throw new IllegalArgumentException(
                    "materialization.mode must be " + MATERIALIZATION_MODE);
        }
        if (!"HermiT".equalsIgnoreCase(configuration.getProperty("engine.id"))) {
            throw new IllegalArgumentException("engine.id must be HermiT");
        }
        if (configuration.getProperty("hermit.version", "").trim().isEmpty()) {
            throw new IllegalArgumentException("hermit.version is required");
        }
        return configuration;
    }

    private static OWLOntology loadOntology(Path ontologyPath) throws Exception {
        OWLOntologyManager manager = OWLManager.createOWLOntologyManager();
        return manager.loadOntologyFromOntologyDocument(ontologyPath.toFile());
    }

    private static Reasoner createReasoner(OWLOntology ontology, Properties configuration)
            throws Exception {
        Configuration reasonerConfiguration = new Configuration();
        return new Reasoner(reasonerConfiguration, ontology);
    }

    private static void printVersion(Path ontologyPath, Path configurationPath) throws Exception {
        Properties configuration = readConfiguration(configurationPath);
        Reasoner reasoner = createReasoner(loadOntology(ontologyPath), configuration);
        try {
            System.out.println("HermiT " + configuration.getProperty("hermit.version").trim());
        } finally {
            reasoner.dispose();
        }
    }

    private static Model parseRdfXml(Path ontologyPath) throws Exception {
        return parse(ontologyPath, RDFFormat.RDFXML);
    }

    private static Model parseNQuads(Path graphPath) throws Exception {
        return parse(graphPath, RDFFormat.NQUADS);
    }

    private static Model parse(Path path, RDFFormat format) throws Exception {
        Model model = new LinkedHashModel();
        RDFParser parser = format == RDFFormat.RDFXML
                ? new RDFXMLParser()
                : new NQuadsParser();
        parser.setPreserveBNodeIDs(true);
        parser.setRDFHandler(new StatementCollector(model));
        try (InputStream input = Files.newInputStream(path)) {
            parser.parse(input, path.toUri().toString());
        }
        return model;
    }

    private static void writeNQuads(Model model, PrintStream output) throws Exception {
        RDFWriter writer = new NQuadsWriter(output);
        writer.startRDF();
        for (org.eclipse.rdf4j.model.Statement statement : model) {
            writer.handleStatement(statement);
        }
        writer.endRDF();
    }

    private static void export(
            Path ontologyPath,
            Path sourceGraphPath,
            Path queryDirectory,
            Path configurationPath,
            PrintStream output)
            throws Exception {
        Properties configuration = readConfiguration(configurationPath);
        Model sourceGraph = parseNQuads(sourceGraphPath);
        Model model = new LinkedHashModel(sourceGraph);
        Model obligations = new LinkedHashModel();
        Set<Value> sourceTerms = collectTerms(sourceGraph);
            Set<String> queryIris = collectQueryIris(queryDirectory, configuration);
            boolean queryClassHierarchy = queryContainsPredicate(
                    queryDirectory, configuration, CLASS_HIERARCHY_PREDICATES);
            boolean queryPropertyHierarchy = queryContainsPredicate(
                    queryDirectory, configuration, PROPERTY_HIERARCHY_PREDICATES);
        requireSourceFamilies(sourceGraph);
        OWLOntology ontology = loadOntology(ontologyPath);
        Reasoner reasoner = createReasoner(ontology, configuration);
        try {
            if (!reasoner.isConsistent()) {
                throw new IllegalStateException("HermiT reported an inconsistent DL ontology");
            }
            reasoner.precomputeInferences(
                    InferenceType.CLASS_HIERARCHY,
                    InferenceType.OBJECT_PROPERTY_HIERARCHY,
                    InferenceType.DATA_PROPERTY_HIERARCHY,
                    InferenceType.CLASS_ASSERTIONS,
                    InferenceType.OBJECT_PROPERTY_ASSERTIONS,
                    InferenceType.DATA_PROPERTY_ASSERTIONS);
            OWLDataFactory factory = ontology.getOWLOntologyManager().getOWLDataFactory();
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals =
                    collectSourceAnonymousIndividuals(factory, sourceGraph, sourceTerms);
            rejectGeneratedObjectWitnessDependencies(
                    reasoner,
                    factory,
                    ontology,
                    queryIris,
                    sourceGraph,
                    sourceAnonymousIndividuals,
                    sourceTerms);
            rejectGeneratedDataWitnessDependencies(
                    reasoner,
                    factory,
                    ontology,
                    queryIris,
                    sourceGraph,
                    sourceAnonymousIndividuals,
                    sourceTerms);
            materializeQueryClasses(
                    reasoner,
                    factory,
                    ontology,
                    queryIris,
                    model,
                    obligations,
                    sourceGraph,
                    sourceAnonymousIndividuals,
                    sourceTerms,
                    queryClassHierarchy);
            materializeQueryProperties(
                    reasoner,
                    factory,
                    ontology,
                    queryIris,
                    model,
                    obligations,
                    sourceGraph,
                    sourceAnonymousIndividuals,
                    sourceTerms,
                    queryPropertyHierarchy);
            writeNQuads(model, output);
            writeObligationArtifacts(
                    obligations,
                    sourceGraphPath,
                    queryDirectory,
                    queryIris,
                    sourceTerms,
                    sourceAnonymousIndividuals);
        } finally {
            reasoner.dispose();
        }
    }

    private static void rejectGeneratedObjectWitnessDependencies(
            Reasoner reasoner,
            OWLDataFactory factory,
            OWLOntology ontology,
            Set<String> queryIris,
            Model sourceGraph,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals,
            Set<Value> sourceTerms)
            throws Exception {
        if (!hasFiniteSourceAssertions(sourceGraph, sourceAnonymousIndividuals)) {
            return;
        }
        for (OWLClassExpression expression : ontology.getNestedClassExpressions()) {
            ObjectWitnessRequirement requirement = requiredObjectRequirement(expression);
            boolean propertyQueried = requirement != null
                    && queryIris.contains(
                            requirement.property.getNamedProperty().getIRI().toString());
            boolean fillerQueried = requirement != null
                    && requirement.filler instanceof OWLClass
                    && queryIris.contains(requirement.filler.asOWLClass().getIRI().toString());
            if (requirement == null || (!propertyQueried && !fillerQueried)) {
                continue;
            }
            OWLObjectPropertyExpression property = requirement.property;
            Set<OWLClass> owners = restrictionOwners(ontology, expression);
            Set<OWLNamedIndividual> namedInstances = new HashSet<>();
            if (owners.isEmpty()) {
                namedInstances.addAll(reasoner.getInstances(expression, false).getFlattened());
            } else {
                for (OWLClass owner : owners) {
                    namedInstances.addAll(reasoner.getInstances(owner, false).getFlattened());
                }
            }
            for (OWLNamedIndividual individual : namedInstances) {
                if (iriResource(individual.getIRI().toString(), sourceTerms) == null) {
                    throw new NotAvailableException(
                            "existential query dependency uses a named individual outside the finite source domain");
                }
                if (!hasFiniteObjectValue(
                        reasoner,
                        factory,
                        requirement,
                        individual,
                        sourceGraph,
                        ontology,
                        sourceAnonymousIndividuals,
                        sourceTerms)) {
                    throw new NotAvailableException(
                            "query dependency requires a generated anonymous witness for "
                                    + property.getNamedProperty().getIRI());
                }
            }
            for (OWLAnonymousIndividual individual : sourceAnonymousIndividuals) {
                Resource subject = anonymousResource(individual, sourceTerms);
                if (subject == null) {
                    continue;
                }
                if (owners.isEmpty()) {
                    throw new NotAvailableException(
                            "query existential restriction has no finite named owner for an input blank node");
                }
                boolean restricted = false;
                for (OWLClass owner : owners) {
                    restricted |= sourceAnonymousClassEntails(
                            reasoner, factory, sourceGraph, subject, owner);
                }
                if (restricted
                        && !hasFiniteObjectValue(
                                reasoner,
                                factory,
                                requirement,
                                individual,
                                sourceGraph,
                                sourceAnonymousIndividuals,
                                ontology,
                                sourceTerms)) {
                    throw new NotAvailableException(
                            "query dependency requires a generated anonymous witness for "
                                    + property.getNamedProperty().getIRI());
                }
            }
        }
    }

    private static Set<OWLClass> restrictionOwners(
            OWLOntology ontology, OWLClassExpression restriction) {
        Set<OWLClass> owners = new HashSet<>();
        for (OWLAxiom axiom : ontology.getAxioms()) {
            if (axiom instanceof OWLEquivalentClassesAxiom) {
                OWLEquivalentClassesAxiom equivalent = (OWLEquivalentClassesAxiom) axiom;
                boolean containsRestriction = false;
                for (OWLClassExpression expression : equivalent.getClassExpressions()) {
                    if (expression.getNestedClassExpressions().contains(restriction)) {
                        containsRestriction = true;
                        break;
                    }
                }
                if (containsRestriction) {
                    owners.addAll(equivalent.getNamedClasses());
                }
                continue;
            }
            if (axiom instanceof OWLSubClassOfAxiom) {
                OWLSubClassOfAxiom subclass = (OWLSubClassOfAxiom) axiom;
                if (subclass.getSubClass() instanceof OWLClass
                        && subclass.getSuperClass().getNestedClassExpressions().contains(restriction)) {
                    owners.add((OWLClass) subclass.getSubClass());
                }
            }
        }
        return owners;
    }

    private static void rejectGeneratedDataWitnessDependencies(
            Reasoner reasoner,
            OWLDataFactory factory,
            OWLOntology ontology,
            Set<String> queryIris,
            Model sourceGraph,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals,
            Set<Value> sourceTerms)
            throws Exception {
        if (!hasFiniteSourceAssertions(sourceGraph, sourceAnonymousIndividuals)) {
            return;
        }
        for (OWLClassExpression expression : ontology.getNestedClassExpressions()) {
            DataWitnessRequirement requirement = requiredDataRequirement(expression);
            if (requirement == null
                    || !queryIris.contains(
                            requirement.property.asOWLDataProperty().getIRI().toString())) {
                continue;
            }
            Set<OWLClass> owners = restrictionOwners(ontology, expression);
            if (owners.isEmpty() && !sourceAnonymousIndividuals.isEmpty()) {
                throw new NotAvailableException(
                        "query data existential restriction has no finite named owner for an input blank node");
            }
            Set<OWLNamedIndividual> namedInstances = new HashSet<>();
            if (owners.isEmpty()) {
                namedInstances.addAll(reasoner.getInstances(expression, false).getFlattened());
            } else {
                for (OWLClass owner : owners) {
                    namedInstances.addAll(reasoner.getInstances(owner, false).getFlattened());
                }
            }
            for (OWLNamedIndividual individual : namedInstances) {
                Resource subject = iriResource(individual.getIRI().toString(), sourceTerms);
                if (subject == null
                        || !hasFiniteDataValue(
                                reasoner,
                                requirement,
                                individual,
                                sourceTerms)) {
                    throw new NotAvailableException(
                            "query dependency requires a generated data witness for "
                                    + requirement.property.asOWLDataProperty().getIRI());
                }
            }
            for (OWLAnonymousIndividual individual : sourceAnonymousIndividuals) {
                Resource subject = anonymousResource(individual, sourceTerms);
                boolean restricted = false;
                if (subject != null) {
                    for (OWLClass owner : owners) {
                        if (sourceAnonymousClassEntails(
                                reasoner, factory, sourceGraph, subject, owner)) {
                            restricted = true;
                            break;
                        }
                    }
                }
                if (restricted) {
                    if (!hasFiniteDataValue(
                            reasoner,
                            factory,
                            requirement,
                            individual,
                            sourceGraph,
                            ontology,
                            sourceTerms)) {
                        throw new NotAvailableException(
                                "query dependency requires a generated data witness for "
                                        + requirement.property.asOWLDataProperty().getIRI());
                    }
                }
            }
        }
    }

    private static DataWitnessRequirement requiredDataRequirement(
            OWLClassExpression expression) {
        if (expression instanceof OWLDataSomeValuesFrom) {
            OWLDataSomeValuesFrom restriction = (OWLDataSomeValuesFrom) expression;
            return new DataWitnessRequirement(restriction.getProperty(), restriction.getFiller(), 1);
        }
        if (expression instanceof OWLDataMinCardinality) {
            OWLDataMinCardinality restriction = (OWLDataMinCardinality) expression;
            return restriction.getCardinality() > 0
                    ? new DataWitnessRequirement(
                            restriction.getProperty(),
                            restriction.getFiller(),
                            restriction.getCardinality())
                    : null;
        }
        if (expression instanceof OWLDataExactCardinality) {
            OWLDataExactCardinality restriction = (OWLDataExactCardinality) expression;
            return restriction.getCardinality() > 0
                    ? new DataWitnessRequirement(
                            restriction.getProperty(),
                            restriction.getFiller(),
                            restriction.getCardinality())
                    : null;
        }
        return null;
    }

    private static boolean hasFiniteDataValue(
            Reasoner reasoner,
            DataWitnessRequirement requirement,
            OWLNamedIndividual subject,
            Set<Value> sourceTerms)
            throws Exception {
        int found = 0;
        for (org.semanticweb.owlapi.model.OWLLiteral literal
                : reasoner.getDataPropertyValues(
                        subject, requirement.property.asOWLDataProperty())) {
            Literal object = rdfLiteral(literal);
            if (!sourceTerms.contains(object)) {
                throw new NotAvailableException(
                        "existential data entailment uses a literal outside the finite source domain");
            }
            if (dataRangeContains(requirement.filler, literal)) {
                found++;
            }
        }
        return found >= requirement.minimumCardinality;
    }

    private static boolean hasFiniteDataValue(
            Reasoner reasoner,
            OWLDataFactory factory,
            DataWitnessRequirement requirement,
            OWLAnonymousIndividual subject,
            Model sourceGraph,
            OWLOntology ontology,
            Set<Value> sourceTerms)
            throws Exception {
        Resource subjectResource = anonymousResource(subject, sourceTerms);
        if (subjectResource == null) {
            return false;
        }
        int found = 0;
        for (org.eclipse.rdf4j.model.Statement statement
                : sourceGraph.filter(subjectResource, null, null)) {
            if (!(statement.getObject() instanceof Literal)
                    || !isAssertionProperty(statement.getPredicate())) {
                continue;
            }
            OWLDataProperty sourceProperty = factory.getOWLDataProperty(
                    org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
            if (dataPropertyEntails(
                            reasoner,
                            sourceProperty,
                            requirement.property.asOWLDataProperty())
                    && dataRangeContains(
                            requirement.filler,
                            owlLiteral(factory, (Literal) statement.getObject()))) {
                found++;
            }
        }
        return found >= requirement.minimumCardinality;
    }

    private static boolean dataRangeContains(
            OWLDataRange range, org.semanticweb.owlapi.model.OWLLiteral literal) {
        if (range.isTopDatatype()) {
            return true;
        }
        if (range instanceof org.semanticweb.owlapi.model.OWLDataOneOf) {
            return ((org.semanticweb.owlapi.model.OWLDataOneOf) range).getValues().contains(literal);
        }
        return range.isDatatype()
                && range.asOWLDatatype().getIRI().equals(literal.getDatatype().getIRI());
    }

    private static boolean sourceAnonymousClassEntails(
            Reasoner reasoner,
            OWLDataFactory factory,
            Model sourceGraph,
            Resource subject,
            OWLClassExpression target)
            throws Exception {
        if (target.isOWLThing()) {
            return true;
        }
        if (!(target instanceof OWLClass)) {
            return false;
        }
        OWLClass targetClass = (OWLClass) target;
        for (org.eclipse.rdf4j.model.Statement statement
                : sourceGraph.filter(subject, RDF.TYPE, null)) {
            if (!(statement.getObject() instanceof IRI)) {
                continue;
            }
            String type = statement.getObject().stringValue();
            if (type.startsWith(OWL.NAMESPACE)
                    || type.startsWith(RDF.NAMESPACE)
                    || type.startsWith(RDFS.NAMESPACE)) {
                continue;
            }
            OWLClass asserted = factory.getOWLClass(
                    org.semanticweb.owlapi.model.IRI.create(type));
            if (classEntails(reasoner, asserted, targetClass)) {
                return true;
            }
        }
        for (org.eclipse.rdf4j.model.Statement statement
                : sourceGraph.filter(subject, null, null)) {
            if (!isAssertionProperty(statement.getPredicate())) {
                continue;
            }
            if (statement.getObject() instanceof Resource) {
                OWLObjectProperty property = factory.getOWLObjectProperty(
                        org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
                for (OWLClass domain : reasoner.getObjectPropertyDomains(property, false).getFlattened()) {
                    if (classEntails(reasoner, domain, targetClass)) {
                        return true;
                    }
                }
            } else if (statement.getObject() instanceof Literal) {
                OWLDataProperty property = factory.getOWLDataProperty(
                        org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
                for (OWLClass domain : reasoner.getDataPropertyDomains(property, false).getFlattened()) {
                    if (classEntails(reasoner, domain, targetClass)) {
                        return true;
                    }
                }
            }
        }
        for (org.eclipse.rdf4j.model.Statement statement
                : sourceGraph.filter(null, null, subject)) {
            if (!(statement.getSubject() instanceof Resource)
                    || !isAssertionProperty(statement.getPredicate())
                    || !(statement.getPredicate() instanceof IRI)) {
                continue;
            }
            OWLObjectProperty property = factory.getOWLObjectProperty(
                    org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
            for (OWLClass range : reasoner.getObjectPropertyRanges(property, false).getFlattened()) {
                if (classEntails(reasoner, range, targetClass)) {
                    return true;
                }
            }
        }
        return false;
    }

    private static boolean classEntails(
            Reasoner reasoner, OWLClass asserted, OWLClass target) throws Exception {
        return asserted.equals(target)
                || reasoner.getSuperClasses(asserted, false).getFlattened().contains(target)
                || reasoner.getEquivalentClasses(asserted).getEntities().contains(target);
    }

    private static ObjectWitnessRequirement requiredObjectRequirement(
            OWLClassExpression expression) {
        if (expression instanceof OWLObjectSomeValuesFrom) {
            OWLObjectSomeValuesFrom restriction = (OWLObjectSomeValuesFrom) expression;
            return new ObjectWitnessRequirement(restriction.getProperty(), restriction.getFiller(), 1);
        }
        if (expression instanceof OWLObjectMinCardinality) {
            OWLObjectMinCardinality restriction = (OWLObjectMinCardinality) expression;
            return restriction.getCardinality() > 0
                    ? new ObjectWitnessRequirement(
                            restriction.getProperty(),
                            restriction.getFiller(),
                            restriction.getCardinality())
                    : null;
        }
        if (expression instanceof OWLObjectExactCardinality) {
            OWLObjectExactCardinality restriction = (OWLObjectExactCardinality) expression;
            return restriction.getCardinality() > 0
                    ? new ObjectWitnessRequirement(
                            restriction.getProperty(),
                            restriction.getFiller(),
                            restriction.getCardinality())
                    : null;
        }
        return null;
    }

    private static boolean hasFiniteObjectValue(
            Reasoner reasoner,
            OWLDataFactory factory,
            ObjectWitnessRequirement requirement,
            OWLNamedIndividual subject,
            Model sourceGraph,
            OWLOntology ontology,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals,
            Set<Value> sourceTerms)
            throws Exception {
        int found = 0;
        for (OWLNamedIndividual value
                : reasoner.getObjectPropertyValues(subject, requirement.property).getFlattened()) {
            if (iriResource(value.getIRI().toString(), sourceTerms) == null) {
                throw new NotAvailableException(
                        "existential entailment uses a named object outside the finite source domain");
            }
            if (reasoner.isEntailed(
                    factory.getOWLClassAssertionAxiom(requirement.filler, value))) {
                found++;
            }
        }
        for (OWLAnonymousIndividual value : sourceAnonymousIndividuals) {
            Resource object = anonymousResource(value, sourceTerms);
            if (object != null
                    && (sourceHasObjectAssertion(
                                    sourceGraph,
                                    iriResource(subject.getIRI().toString(), sourceTerms),
                                    requirement.property,
                                    object)
                            || finiteSourceObjectPropertyExpressionSupport(
                                    reasoner,
                                    factory,
                                    ontology,
                                    sourceGraph,
                                    requirement.property,
                                    iriResource(subject.getIRI().toString(), sourceTerms),
                                    object,
                                    sourceTerms))
                    && sourceAnonymousClassEntails(
                            reasoner, factory, sourceGraph, object, requirement.filler)) {
                found++;
            }
        }
        return found >= requirement.minimumCardinality;
    }

    private static boolean hasFiniteObjectValue(
            Reasoner reasoner,
            OWLDataFactory factory,
            ObjectWitnessRequirement requirement,
            OWLAnonymousIndividual subject,
            Model sourceGraph,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals,
            OWLOntology ontology,
            Set<Value> sourceTerms)
            throws Exception {
        int found = 0;
        Resource subjectResource = anonymousResource(subject, sourceTerms);
        if (subjectResource == null) {
            return false;
        }
        for (OWLNamedIndividual value : ontology.getIndividualsInSignature()) {
            Resource object = iriResource(value.getIRI().toString(), sourceTerms);
            if (object != null
                    && (sourceHasObjectAssertion(
                                    sourceGraph, subjectResource, requirement.property, object)
                            || finiteSourceObjectPropertyExpressionSupport(
                                    reasoner,
                                    factory,
                                    ontology,
                                    sourceGraph,
                                    requirement.property,
                                    subjectResource,
                                    object,
                                    sourceTerms))
                    && reasoner.isEntailed(
                            factory.getOWLClassAssertionAxiom(requirement.filler, value))) {
                found++;
            }
        }
        for (OWLAnonymousIndividual value : sourceAnonymousIndividuals) {
            Resource object = anonymousResource(value, sourceTerms);
            if (object != null
                    && (sourceHasObjectAssertion(
                                    sourceGraph, subjectResource, requirement.property, object)
                            || finiteSourceObjectPropertyExpressionSupport(
                                    reasoner,
                                    factory,
                                    ontology,
                                    sourceGraph,
                                    requirement.property,
                                    subjectResource,
                                    object,
                                    sourceTerms))
                    && sourceAnonymousClassEntails(
                            reasoner, factory, sourceGraph, object, requirement.filler)) {
                found++;
            }
        }
        return found >= requirement.minimumCardinality;
    }

    private static boolean sourceHasObjectAssertion(
            Model sourceGraph,
            Resource subject,
            OWLObjectPropertyExpression property,
            Resource object) {
        if (subject == null || object == null) {
            return false;
        }
        IRI namedProperty = SimpleValueFactory.getInstance().createIRI(
                property.getNamedProperty().getIRI().toString());
        if (property.isAnonymous()) {
            return sourceGraph.contains(object, namedProperty, subject);
        }
        return sourceGraph.contains(subject, namedProperty, object);
    }

    private static boolean hasFiniteSourceAssertions(
            Model sourceGraph, Set<OWLAnonymousIndividual> sourceAnonymousIndividuals) {
        if (!sourceAnonymousIndividuals.isEmpty()) {
            return true;
        }
        for (org.eclipse.rdf4j.model.Statement statement : sourceGraph) {
            String predicate = statement.getPredicate().stringValue();
            if (RDF.TYPE.stringValue().equals(predicate)
                    && statement.getObject() instanceof IRI
                    && !statement.getObject().stringValue().startsWith(OWL.NAMESPACE)
                    && !statement.getObject().stringValue().startsWith(RDF.NAMESPACE)
                    && !statement.getObject().stringValue().startsWith(RDFS.NAMESPACE)) {
                return true;
            }
            if (!RDF.TYPE.stringValue().equals(predicate)
                    && !RDF.FIRST.stringValue().equals(predicate)
                    && !RDF.REST.stringValue().equals(predicate)
                    && !predicate.startsWith(OWL.NAMESPACE)
                    && !predicate.startsWith(RDF.NAMESPACE)
                    && !predicate.startsWith(RDFS.NAMESPACE)) {
                return true;
            }
        }
        return false;
    }

    private static void requireSourceFamilies(Model model) throws NotAvailableException {
        boolean classAssertions = false;
        boolean objectAssertions = false;
        boolean dataAssertions = false;
        boolean tboxAssertions = false;
        boolean rboxAssertions = false;
        Set<String> schemaPredicates = new HashSet<>(
                Arrays.asList(
                        RDF.TYPE.stringValue(),
                        RDF.FIRST.stringValue(),
                        RDF.REST.stringValue(),
                        RDFS.LABEL.stringValue(),
                        RDFS.COMMENT.stringValue()));
        Set<String> tboxPredicates = new HashSet<>(
                Arrays.asList(
                        RDFS.SUBCLASSOF.stringValue(),
                        RDFS.DOMAIN.stringValue(),
                        RDFS.RANGE.stringValue(),
                        OWL.EQUIVALENTCLASS.stringValue(),
                        OWL.DISJOINTWITH.stringValue(),
                        OWL.COMPLEMENTOF.stringValue(),
                        OWL.UNIONOF.stringValue(),
                        OWL.INTERSECTIONOF.stringValue(),
                        OWL.ONPROPERTY.stringValue(),
                        OWL.SOMEVALUESFROM.stringValue(),
                        OWL.ALLVALUESFROM.stringValue()));
        Set<String> rboxPredicates = new HashSet<>(
                Arrays.asList(
                        RDFS.SUBPROPERTYOF.stringValue(),
                        OWL.EQUIVALENTPROPERTY.stringValue(),
                        OWL.INVERSEOF.stringValue(),
                        OWL.DISJOINTWITH.stringValue()));
        for (org.eclipse.rdf4j.model.Statement statement : model) {
            String predicate = statement.getPredicate().stringValue();
            if (RDF.TYPE.stringValue().equals(predicate)) {
                classAssertions = true;
            }
            if (tboxPredicates.contains(predicate)) {
                tboxAssertions = true;
            }
            if (rboxPredicates.contains(predicate)) {
                rboxAssertions = true;
            }
            if (!schemaPredicates.contains(predicate)
                    && !tboxPredicates.contains(predicate)
                    && !rboxPredicates.contains(predicate)) {
                if (statement.getObject() instanceof Literal) {
                    dataAssertions = true;
                } else {
                    objectAssertions = true;
                }
            }
        }
        List<String> missing = new java.util.ArrayList<>();
        if (!classAssertions) {
            missing.add("class");
        }
        if (!objectAssertions) {
            missing.add("object_property");
        }
        if (!dataAssertions) {
            missing.add("data_property");
        }
        if (!tboxAssertions) {
            missing.add("tbox");
        }
        if (!rboxAssertions) {
            missing.add("rbox");
        }
        if (!missing.isEmpty()) {
            throw new NotAvailableException(
                    "source graph lacks required assertion families: " + String.join(",", missing));
        }
    }

    private static void writeObligationArtifacts(
            Model obligations,
            Path sourceGraphPath,
            Path queryDirectory,
            Set<String> queryIris,
            Set<Value> sourceTerms,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals)
            throws Exception {
        String graphValue = System.getenv("HERMIT_DL_OBLIGATION_OUTPUT");
        String manifestValue = System.getenv("HERMIT_DL_OBLIGATION_MANIFEST");
        if (graphValue == null && manifestValue == null) {
            return;
        }
        if (graphValue == null || manifestValue == null) {
            throw new IllegalArgumentException(
                    "HERMIT_DL_OBLIGATION_OUTPUT and HERMIT_DL_OBLIGATION_MANIFEST must be set together");
        }
        Path graphPath = Paths.get(graphValue).toAbsolutePath().normalize();
        Path manifestPath = Paths.get(manifestValue).toAbsolutePath().normalize();
        Files.createDirectories(graphPath.getParent());
        Files.createDirectories(manifestPath.getParent());
        try (PrintStream graphOutput = new PrintStream(
                new FileOutputStream(graphPath.toFile()), true, StandardCharsets.UTF_8.name())) {
            writeNQuads(obligations, graphOutput);
        }
        StringBuilder json = new StringBuilder();
        json.append("{\n");
        json.append("  \"schema_version\": 1,\n");
        json.append("  \"manifest_id\": \"hermit-dl-obligations\",\n");
        json.append("  \"profile\": \"DL\",\n");
        json.append("  \"status\": \"complete\",\n");
        json.append("  \"closure_scope\": \"finite_query_complete\",\n");
        json.append("  \"anonymous_witness_policy\": \"not_exported\",\n");
        json.append("  \"required_assertion_families\": [\"class\", \"object_property\", \"data_property\", \"tbox\", \"rbox\"],\n");
        json.append("  \"coverage\": {\"strategy\": \"finite_source_domain_query_scope\", \"exhaustive\": true, \"source_term_count\": ")
                .append(sourceTerms.size())
                .append(", \"source_resource_count\": ")
                .append(sourceResourceCount(sourceTerms))
                .append(", \"input_blank_node_count\": ")
                .append(sourceAnonymousIndividuals.size())
                .append(", \"query_iri_count\": ")
                .append(queryIris.size())
                .append("},\n");
        json.append("  \"source_graph\": {\"path\": ")
                .append(jsonString(sourceGraphPath.toAbsolutePath().normalize().toString()))
                .append(", \"sha256\": ")
                .append(jsonString(fileSha256(sourceGraphPath)))
                .append("},\n");
        json.append("  \"obligation_graph\": {\"path\": ")
                .append(jsonString(graphPath.toString()))
                .append(", \"sha256\": ")
                .append(jsonString(fileSha256(graphPath)))
                .append(", \"count\": ")
                .append(obligations.size())
                .append("},\n");
        json.append("  \"query_scope\": {\"directory\": ")
                .append(jsonString(queryDirectory.toAbsolutePath().normalize().toString()))
                .append(", \"query_iris\": [");
        boolean first = true;
        for (String queryIri : queryIris) {
            if (!first) {
                json.append(", ");
            }
            json.append(jsonString(queryIri));
            first = false;
        }
        json.append("]}\n");
        json.append("}\n");
        Files.write(manifestPath, json.toString().getBytes(StandardCharsets.UTF_8));
    }

    private static int sourceResourceCount(Set<Value> sourceTerms) {
        int count = 0;
        for (Value term : sourceTerms) {
            if (term instanceof Resource) {
                count++;
            }
        }
        return count;
    }

    private static String jsonString(String value) {
        StringBuilder escaped = new StringBuilder("\"");
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            switch (character) {
                case '\\':
                    escaped.append("\\\\");
                    break;
                case '"':
                    escaped.append("\\\"");
                    break;
                case '\n':
                    escaped.append("\\n");
                    break;
                case '\r':
                    escaped.append("\\r");
                    break;
                case '\t':
                    escaped.append("\\t");
                    break;
                default:
                    escaped.append(character);
                    break;
            }
        }
        return escaped.append('"').toString();
    }

    private static String fileSha256(Path path) throws Exception {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream input = Files.newInputStream(path)) {
            byte[] buffer = new byte[8192];
            int read;
            while ((read = input.read(buffer)) >= 0) {
                if (read > 0) {
                    digest.update(buffer, 0, read);
                }
            }
        }
        StringBuilder result = new StringBuilder("sha256:");
        for (byte value : digest.digest()) {
            result.append(String.format("%02x", value & 0xff));
        }
        return result.toString();
    }

    private static Set<Value> collectTerms(Model model) {
        Set<Value> terms = new HashSet<>();
        for (org.eclipse.rdf4j.model.Statement statement : model) {
            terms.add(statement.getSubject());
            terms.add(statement.getPredicate());
            terms.add(statement.getObject());
            if (statement.getContext() != null) {
                terms.add(statement.getContext());
            }
        }
        return terms;
    }

    private static Set<OWLAnonymousIndividual> collectSourceAnonymousIndividuals(
            OWLDataFactory factory, Model sourceGraph, Set<Value> sourceTerms) {
        Set<OWLAnonymousIndividual> individuals = new HashSet<>();
        for (Value term : sourceTerms) {
            if (!(term instanceof BNode)) {
                continue;
            }
            Resource resource = (Resource) term;
            boolean hasABoxEvidence = false;
            for (org.eclipse.rdf4j.model.Statement statement : sourceGraph.filter(resource, null, null)) {
                String predicate = statement.getPredicate().stringValue();
                if (RDF.TYPE.stringValue().equals(predicate)
                        && statement.getObject() instanceof IRI
                        && !statement.getObject().stringValue().startsWith(OWL.NAMESPACE)
                        && !statement.getObject().stringValue().startsWith(RDF.NAMESPACE)
                        && !statement.getObject().stringValue().startsWith(RDFS.NAMESPACE)) {
                    hasABoxEvidence = true;
                    break;
                }
                if (!RDF.TYPE.stringValue().equals(predicate)
                        && !RDF.FIRST.stringValue().equals(predicate)
                        && !RDF.REST.stringValue().equals(predicate)
                        && !predicate.startsWith(OWL.NAMESPACE)
                        && !predicate.startsWith(RDF.NAMESPACE)
                        && !predicate.startsWith(RDFS.NAMESPACE)) {
                    hasABoxEvidence = true;
                    break;
                }
            }
            if (!hasABoxEvidence) {
                for (org.eclipse.rdf4j.model.Statement statement : sourceGraph.filter(null, null, resource)) {
                    String predicate = statement.getPredicate().stringValue();
                    if (!predicate.startsWith(OWL.NAMESPACE)
                            && !predicate.startsWith(RDF.NAMESPACE)
                            && !predicate.startsWith(RDFS.NAMESPACE)) {
                        hasABoxEvidence = true;
                        break;
                    }
                }
            }
            if (hasABoxEvidence) {
                individuals.add(factory.getOWLAnonymousIndividual(((BNode) term).getID()));
            }
        }
        return individuals;
    }

    private static Set<String> collectQueryIris(Path queryDirectory, Properties configuration)
            throws IOException {
        if (!Files.isDirectory(queryDirectory)) {
            throw new IOException("query directory is missing: " + queryDirectory);
        }
        String namespace = configuration.getProperty("query.namespace", OWL2BENCH_NAMESPACE);
        Set<String> iris = new TreeSet<>();
        Map<String, String> prefixes = new HashMap<>();
        prefixes.put("", namespace);
        prefixes.put("rdf", RDF.NAMESPACE);
        prefixes.put("rdfs", RDFS.NAMESPACE);
        prefixes.put("owl", OWL.NAMESPACE);
        prefixes.put("rdf", RDF.NAMESPACE);
        prefixes.put("rdfs", RDFS.NAMESPACE);
        prefixes.put("owl", OWL.NAMESPACE);
        try (java.nio.file.DirectoryStream<Path> files = Files.newDirectoryStream(
                queryDirectory, "*.rq")) {
            for (Path query : files) {
                String text = new String(Files.readAllBytes(query), StandardCharsets.UTF_8);
                Matcher declarationMatcher = PREFIX_DECLARATION.matcher(text);
                while (declarationMatcher.find()) {
                    String prefix = declarationMatcher.group(1);
                    prefixes.put(prefix == null ? "" : prefix, declarationMatcher.group(2));
                }
                Matcher prefixedNameMatcher = PREFIXED_NAME.matcher(text);
                while (prefixedNameMatcher.find()) {
                    String base = prefixes.get(
                            prefixedNameMatcher.group(1) == null
                                    ? ""
                                    : prefixedNameMatcher.group(1));
                    if (base != null) {
                        iris.add(base + prefixedNameMatcher.group(2));
                    }
                }
                Matcher iriMatcher = FULL_IRI.matcher(text);
                while (iriMatcher.find()) {
                    iris.add(iriMatcher.group(1));
                }
            }
        }
        if (iris.isEmpty()) {
            throw new IOException("query directory contains no query IRIs: " + queryDirectory);
        }
        return iris;
    }

    private static boolean queryContainsPredicate(
            Path queryDirectory, Properties configuration, Set<String> predicates)
            throws IOException {
        String namespace = configuration.getProperty("query.namespace", OWL2BENCH_NAMESPACE);
        try (java.nio.file.DirectoryStream<Path> files = Files.newDirectoryStream(
                queryDirectory, "*.rq")) {
            for (Path query : files) {
                String text = new String(Files.readAllBytes(query), StandardCharsets.UTF_8);
                Map<String, String> prefixes = new HashMap<>();
                prefixes.put("", namespace);
                prefixes.put("rdf", RDF.NAMESPACE);
                prefixes.put("rdfs", RDFS.NAMESPACE);
                prefixes.put("owl", OWL.NAMESPACE);
                prefixes.put("rdf", RDF.NAMESPACE);
                prefixes.put("rdfs", RDFS.NAMESPACE);
                prefixes.put("owl", OWL.NAMESPACE);
                Matcher declarationMatcher = PREFIX_DECLARATION.matcher(text);
                while (declarationMatcher.find()) {
                    String prefix = declarationMatcher.group(1);
                    prefixes.put(prefix == null ? "" : prefix, declarationMatcher.group(2));
                }
                Matcher prefixedNameMatcher = PREFIXED_NAME.matcher(text);
                while (prefixedNameMatcher.find()) {
                    String base = prefixes.get(
                            prefixedNameMatcher.group(1) == null
                                    ? ""
                                    : prefixedNameMatcher.group(1));
                    if (base != null && predicates.contains(base + prefixedNameMatcher.group(2))) {
                        return true;
                    }
                }
                Matcher iriMatcher = FULL_IRI.matcher(text);
                while (iriMatcher.find()) {
                    if (predicates.contains(iriMatcher.group(1))) {
                        return true;
                    }
                }
            }
        }
        return false;
    }

    private static void materializeQueryClasses(
            Reasoner reasoner,
            OWLDataFactory factory,
            OWLOntology ontology,
            Set<String> queryIris,
            Model model,
            Model obligations,
            Model sourceGraph,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals,
            Set<Value> sourceTerms,
            boolean includeAllClassHierarchies)
            throws Exception {
        Set<OWLClass> queriedClasses = new HashSet<>();
        for (String iri : queryIris) {
            OWLClass owlClass = factory.getOWLClass(org.semanticweb.owlapi.model.IRI.create(iri));
            if (ontology.getClassesInSignature().contains(owlClass)) {
                queriedClasses.add(owlClass);
            }
        }
        for (OWLClass owlClass : queriedClasses) {
            String iri = owlClass.getIRI().toString();
            NodeSet<OWLNamedIndividual> instances = reasoner.getInstances(owlClass, false);
            for (OWLNamedIndividual individual : instances.getFlattened()) {
                Resource subject = iriResource(individual.getIRI().toString(), sourceTerms);
                IRI object = iriValue(iri, sourceTerms);
                if (subject == null || object == null) {
                    throw new NotAvailableException(
                            "class entailment uses a term outside the finite source domain");
                }
                addInference(model, obligations, subject, RDF.TYPE, object);
            }
            for (OWLAnonymousIndividual individual : sourceAnonymousIndividuals) {
                Resource subject = anonymousResource(individual, sourceTerms);
                IRI object = iriValue(iri, sourceTerms);
                if (subject == null || object == null) {
                    throw new NotAvailableException(
                            "class entailment uses an unmapped anonymous source individual");
                }
                if (sourceAnonymousClassEntails(
                        reasoner, factory, sourceGraph, subject, owlClass)) {
                    addInference(model, obligations, subject, RDF.TYPE, object);
                }
            }
        }
        Set<OWLClass> hierarchyClasses = new HashSet<>(queriedClasses);
        if (includeAllClassHierarchies) {
            hierarchyClasses.addAll(ontology.getClassesInSignature());
        }
        for (OWLClass owlClass : hierarchyClasses) {
            addClassHierarchy(reasoner, owlClass, model, obligations, sourceTerms);
        }
    }

    private static void materializeQueryProperties(
            Reasoner reasoner,
            OWLDataFactory factory,
            OWLOntology ontology,
            Set<String> queryIris,
            Model model,
            Model obligations,
            Model sourceGraph,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals,
            Set<Value> sourceTerms,
            boolean includeAllPropertyHierarchies)
        throws Exception {
        Set<OWLNamedIndividual> individuals = ontology.getIndividualsInSignature();
        Set<OWLAnonymousIndividual> anonymousIndividuals = sourceAnonymousIndividuals;
        for (String iri : queryIris) {
            OWLObjectProperty objectProperty =
                    factory.getOWLObjectProperty(org.semanticweb.owlapi.model.IRI.create(iri));
            if (ontology.getObjectPropertiesInSignature().contains(objectProperty)) {
                addFiniteObjectPairEntailments(
                        reasoner,
                        factory,
                        objectProperty,
                        ontology,
                        sourceGraph,
                        sourceAnonymousIndividuals,
                        model,
                        obligations,
                        sourceTerms);
                for (OWLNamedIndividual individual : individuals) {
                    Resource subject = iriResource(individual.getIRI().toString(), sourceTerms);
                    if (subject == null) {
                        throw new NotAvailableException(
                                "object-property entailment uses a subject outside the finite source domain");
                    }
                    NodeSet<OWLNamedIndividual> values =
                            reasoner.getObjectPropertyValues(individual, objectProperty);
                    for (OWLNamedIndividual value : values.getFlattened()) {
                        Resource object = iriResource(value.getIRI().toString(), sourceTerms);
                        if (object != null) {
                            addInference(
                                    model,
                                    obligations,
                                    subject,
                                    iriValue(iri, sourceTerms),
                                    object);
                        } else {
                            throw new NotAvailableException(
                                    "object-property entailment uses a term outside the finite source domain");
                        }
                    }
                }
                addObjectPropertyHierarchy(reasoner, objectProperty, model, obligations, sourceTerms);
                continue;
            }
            OWLDataProperty dataProperty =
                    factory.getOWLDataProperty(org.semanticweb.owlapi.model.IRI.create(iri));
            if (!ontology.getDataPropertiesInSignature().contains(dataProperty)) {
                continue;
            }
            for (OWLNamedIndividual individual : individuals) {
                Resource subject = iriResource(individual.getIRI().toString(), sourceTerms);
                if (subject == null) {
                    throw new NotAvailableException(
                            "data-property entailment uses a subject outside the finite source domain");
                }
                for (org.semanticweb.owlapi.model.OWLLiteral literal
                        : reasoner.getDataPropertyValues(individual, dataProperty)) {
                    Value object = rdfLiteral(literal);
                    if (!sourceTerms.contains(object)) {
                        throw new NotAvailableException(
                                "data-property entailment uses a literal outside the finite source domain");
                    }
                    addInference(
                            model,
                            obligations,
                            subject,
                            iriValue(iri, sourceTerms),
                            object);
                }
            }
            for (OWLAnonymousIndividual individual : anonymousIndividuals) {
                Resource subject = anonymousResource(individual, sourceTerms);
                if (subject == null) {
                    continue;
                }
                for (org.eclipse.rdf4j.model.Statement statement
                        : sourceGraph.filter(subject, null, null)) {
                    if (!(statement.getObject() instanceof Literal)
                            || !isAssertionProperty(statement.getPredicate())) {
                        continue;
                    }
                    OWLDataProperty sourceProperty = factory.getOWLDataProperty(
                            org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
                    if (dataPropertyEntails(reasoner, sourceProperty, dataProperty)
                            && !model.contains(subject, iriValue(iri, sourceTerms), statement.getObject())) {
                        addInference(
                                model,
                                obligations,
                                subject,
                                iriValue(iri, sourceTerms),
                                statement.getObject());
                    }
                }
            }
            addDataPropertyHierarchy(
                    reasoner, dataProperty, ontology, model, obligations, sourceTerms);
        }
        if (includeAllPropertyHierarchies) {
            for (OWLObjectProperty property : ontology.getObjectPropertiesInSignature()) {
                addObjectPropertyHierarchy(reasoner, property, model, obligations, sourceTerms);
            }
            for (OWLDataProperty property : ontology.getDataPropertiesInSignature()) {
                addDataPropertyHierarchy(
                        reasoner, property, ontology, model, obligations, sourceTerms);
            }
        }
    }

    private static void addFiniteObjectPairEntailments(
            Reasoner reasoner,
            OWLDataFactory factory,
            OWLObjectProperty property,
            OWLOntology ontology,
            Model sourceGraph,
            Set<OWLAnonymousIndividual> sourceAnonymousIndividuals,
            Model model,
            Model obligations,
            Set<Value> sourceTerms)
            throws Exception {
        if (sourceAnonymousIndividuals.isEmpty()) {
            return;
        }
        Set<Resource> sourceBlankNodes = new HashSet<>();
        for (OWLAnonymousIndividual individual : sourceAnonymousIndividuals) {
            Resource resource = anonymousResource(individual, sourceTerms);
            if (resource != null) {
                sourceBlankNodes.add(resource);
            }
        }
        Set<Resource> finiteResources = new HashSet<>();
        for (Value term : sourceTerms) {
            if (term instanceof IRI) {
                finiteResources.add((Resource) term);
            }
        }
        finiteResources.addAll(sourceBlankNodes);
        IRI predicate = iriValue(property.getIRI().toString(), sourceTerms);
        if (predicate == null) {
            throw new NotAvailableException(
                    "object-property entailment uses a property outside the finite source domain");
        }
        for (Resource subject : finiteResources) {
            for (Resource object : finiteResources) {
                if (!(sourceBlankNodes.contains(subject) || sourceBlankNodes.contains(object))) {
                    continue;
                }
                if (!finiteSourceObjectPropertySupport(
                        reasoner,
                        factory,
                        ontology,
                        sourceGraph,
                        property,
                        subject,
                        object,
                        sourceTerms)) {
                    continue;
                }
                if (!model.contains(subject, predicate, object)) {
                    addInference(model, obligations, subject, predicate, object);
                }
            }
        }
    }

    private static boolean finiteSourceObjectPropertySupport(
            Reasoner reasoner,
            OWLDataFactory factory,
            OWLOntology ontology,
            Model sourceGraph,
            OWLObjectProperty queryProperty,
            Resource subject,
            Resource object,
            Set<Value> sourceTerms)
            throws Exception {
        if (sourceHasObjectAssertion(sourceGraph, subject, queryProperty, object)) {
            return true;
        }
        for (org.eclipse.rdf4j.model.Statement statement
                : sourceGraph.filter(subject, null, object)) {
            if (!isAssertionProperty(statement.getPredicate())) {
                continue;
            }
            OWLObjectProperty sourceProperty = factory.getOWLObjectProperty(
                    org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
            if (objectPropertyEntails(reasoner, sourceProperty, queryProperty)) {
                return true;
            }
        }
        for (org.eclipse.rdf4j.model.Statement statement
                : sourceGraph.filter(object, null, subject)) {
            if (!isAssertionProperty(statement.getPredicate())) {
                continue;
            }
            OWLObjectProperty sourceProperty = factory.getOWLObjectProperty(
                    org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
            if (inverseObjectPropertyEntails(reasoner, sourceProperty, queryProperty)) {
                return true;
            }
        }
        Set<Resource> finiteResources = new HashSet<>();
        for (Value term : sourceTerms) {
            if (term instanceof IRI || term instanceof BNode) {
                finiteResources.add((Resource) term);
            }
        }
        for (OWLObjectProperty transitiveProperty : ontology.getObjectPropertiesInSignature()) {
            if (transitiveProperty.isTransitive(ontology)
                    && objectPropertyEntails(reasoner, transitiveProperty, queryProperty)
                    && finiteTransitiveSupport(
                            reasoner,
                            factory,
                            sourceGraph,
                            transitiveProperty,
                            subject,
                            object,
                            finiteResources)) {
                return true;
            }
        }
        for (OWLAxiom axiom : ontology.getRBoxAxioms(true)) {
            if (!(axiom instanceof OWLSubPropertyChainOfAxiom)) {
                continue;
            }
            OWLSubPropertyChainOfAxiom chainAxiom = (OWLSubPropertyChainOfAxiom) axiom;
            OWLObjectPropertyExpression superProperty = chainAxiom.getSuperProperty();
            if (superProperty.isAnonymous()
                    || !objectPropertyEntails(
                            reasoner, superProperty.asOWLObjectProperty(), queryProperty)
                    || !finiteChainSupport(
                            reasoner,
                            factory,
                            sourceGraph,
                            chainAxiom.getPropertyChain(),
                            0,
                            subject,
                            object,
                            finiteResources)) {
                continue;
            }
            return true;
        }
        return false;
    }

    private static boolean finiteSourceObjectPropertyExpressionSupport(
            Reasoner reasoner,
            OWLDataFactory factory,
            OWLOntology ontology,
            Model sourceGraph,
            OWLObjectPropertyExpression property,
            Resource subject,
            Resource object,
            Set<Value> sourceTerms)
            throws Exception {
        if (property.isAnonymous()) {
            return finiteSourceObjectPropertySupport(
                    reasoner,
                    factory,
                    ontology,
                    sourceGraph,
                    property.getNamedProperty(),
                    object,
                    subject,
                    sourceTerms);
        }
        return finiteSourceObjectPropertySupport(
                reasoner,
                factory,
                ontology,
                sourceGraph,
                property.asOWLObjectProperty(),
                subject,
                object,
                sourceTerms);
    }

    private static boolean finiteTransitiveSupport(
            Reasoner reasoner,
            OWLDataFactory factory,
            Model sourceGraph,
            OWLObjectProperty property,
            Resource subject,
            Resource object,
            Set<Resource> finiteResources)
            throws Exception {
        Set<Resource> reached = new HashSet<>();
        List<Resource> pending = new java.util.ArrayList<>();
        reached.add(subject);
        pending.add(subject);
        for (int index = 0; index < pending.size(); index++) {
            Resource current = pending.get(index);
            for (Resource next : finiteResources) {
                if (reached.contains(next)
                        || !finiteDirectRelationSupport(
                                reasoner, factory, sourceGraph, property, current, next)) {
                    continue;
                }
                if (next.equals(object)) {
                    return true;
                }
                reached.add(next);
                pending.add(next);
            }
        }
        return false;
    }

    private static boolean finiteChainSupport(
            Reasoner reasoner,
            OWLDataFactory factory,
            Model sourceGraph,
            List<OWLObjectPropertyExpression> chain,
            int index,
            Resource subject,
            Resource object,
            Set<Resource> finiteResources)
            throws Exception {
        if (index == chain.size()) {
            return subject.equals(object);
        }
        for (Resource next : finiteResources) {
            if (finiteDirectRelationSupport(
                            reasoner,
                            factory,
                            sourceGraph,
                            chain.get(index),
                            subject,
                            next)
                    && finiteChainSupport(
                            reasoner,
                            factory,
                            sourceGraph,
                            chain,
                            index + 1,
                            next,
                            object,
                            finiteResources)) {
                return true;
            }
        }
        return false;
    }

    private static boolean finiteDirectRelationSupport(
            Reasoner reasoner,
            OWLDataFactory factory,
            Model sourceGraph,
            OWLObjectPropertyExpression target,
            Resource subject,
            Resource object)
            throws Exception {
        if (target.isAnonymous()) {
            for (org.eclipse.rdf4j.model.Statement statement
                    : sourceGraph.filter(object, null, subject)) {
                if (!isAssertionProperty(statement.getPredicate())) {
                    continue;
                }
                OWLObjectProperty sourceProperty = factory.getOWLObjectProperty(
                        org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
                if (inverseObjectPropertyEntails(
                        reasoner, sourceProperty, target.getNamedProperty())) {
                    return true;
                }
            }
            return false;
        }
        for (org.eclipse.rdf4j.model.Statement statement
                : sourceGraph.filter(subject, null, object)) {
            if (!isAssertionProperty(statement.getPredicate())) {
                continue;
            }
            OWLObjectProperty sourceProperty = factory.getOWLObjectProperty(
                    org.semanticweb.owlapi.model.IRI.create(statement.getPredicate().stringValue()));
            if (objectPropertyEntails(
                    reasoner, sourceProperty, target.asOWLObjectProperty())) {
                return true;
            }
        }
        return false;
    }

    private static boolean isAssertionProperty(IRI predicate) {
        String value = predicate.stringValue();
        return !value.startsWith(OWL.NAMESPACE)
                && !value.startsWith(RDF.NAMESPACE)
                && !value.startsWith(RDFS.NAMESPACE);
    }

    private static boolean objectPropertyEntails(
            Reasoner reasoner, OWLObjectProperty source, OWLObjectProperty target)
            throws Exception {
        return source.equals(target)
                || reasoner.getSuperObjectProperties(source, false).getFlattened().contains(target)
                || reasoner.getEquivalentObjectProperties(source).getEntities().contains(target);
    }

    private static boolean dataPropertyEntails(
            Reasoner reasoner, OWLDataProperty source, OWLDataProperty target)
            throws Exception {
        return source.equals(target)
                || reasoner.getSuperDataProperties(source, false).getFlattened().contains(target)
                || reasoner.getEquivalentDataProperties(source).getEntities().contains(target);
    }

    private static boolean inverseObjectPropertyEntails(
            Reasoner reasoner, OWLObjectProperty source, OWLObjectProperty target)
            throws Exception {
        for (OWLObjectPropertyExpression inverse
                : reasoner.getInverseObjectProperties(source).getEntities()) {
            if (!inverse.isAnonymous()
                    && objectPropertyEntails(reasoner, inverse.asOWLObjectProperty(), target)) {
                return true;
            }
        }
        return false;
    }

    private static void addClassHierarchy(
            Reasoner reasoner,
            OWLClass owlClass,
            Model model,
            Model obligations,
            Set<Value> sourceTerms)
            throws Exception {
        IRI subject = iriValue(owlClass.getIRI().toString(), sourceTerms);
        if (subject == null) {
            return;
        }
        for (OWLClass parent : reasoner.getSuperClasses(owlClass, false).getFlattened()) {
            IRI object = iriValue(parent.getIRI().toString(), sourceTerms);
            if (object != null) {
                addInference(model, obligations, subject, RDFS.SUBCLASSOF, object);
            }
        }
        for (OWLClass equivalent : reasoner.getEquivalentClasses(owlClass).getEntities()) {
            IRI object = iriValue(equivalent.getIRI().toString(), sourceTerms);
            if (object != null && !object.equals(subject)) {
                addInference(model, obligations, subject, OWL.EQUIVALENTCLASS, object);
            }
        }
    }

    private static void addObjectPropertyHierarchy(
            Reasoner reasoner,
            OWLObjectProperty property,
            Model model,
            Model obligations,
            Set<Value> sourceTerms)
            throws Exception {
        IRI subject = iriValue(property.getIRI().toString(), sourceTerms);
        if (subject == null) {
            return;
        }
        for (OWLObjectPropertyExpression parent
                : reasoner.getSuperObjectProperties(property, false).getFlattened()) {
            if (parent.isAnonymous()) {
                continue;
            }
            IRI object = iriValue(parent.asOWLObjectProperty().getIRI().toString(), sourceTerms);
            if (object != null) {
                addInference(model, obligations, subject, RDFS.SUBPROPERTYOF, object);
            }
        }
        for (OWLObjectPropertyExpression equivalent
                : reasoner.getEquivalentObjectProperties(property).getEntities()) {
            if (equivalent.isAnonymous()) {
                continue;
            }
            IRI object = iriValue(equivalent.asOWLObjectProperty().getIRI().toString(), sourceTerms);
            if (object != null && !object.equals(subject)) {
                addInference(model, obligations, subject, OWL.EQUIVALENTPROPERTY, object);
            }
        }
        for (OWLObjectPropertyExpression inverse
                : reasoner.getInverseObjectProperties(property).getEntities()) {
            if (inverse.isAnonymous()) {
                continue;
            }
            IRI object = iriValue(inverse.asOWLObjectProperty().getIRI().toString(), sourceTerms);
            if (object != null && !object.equals(subject)) {
                addInference(model, obligations, subject, OWL.INVERSEOF, object);
            }
        }
        addDomainsAndRanges(reasoner, property, subject, model, obligations, sourceTerms);
    }

    private static void addDomainsAndRanges(
            Reasoner reasoner,
            OWLObjectProperty property,
            IRI subject,
            Model model,
            Model obligations,
            Set<Value> sourceTerms)
            throws Exception {
        for (OWLClass domain : reasoner.getObjectPropertyDomains(property, false).getFlattened()) {
            IRI object = iriValue(domain.getIRI().toString(), sourceTerms);
            if (object != null) {
                addInference(model, obligations, subject, RDFS.DOMAIN, object);
            }
        }
        for (OWLClass range : reasoner.getObjectPropertyRanges(property, false).getFlattened()) {
            IRI object = iriValue(range.getIRI().toString(), sourceTerms);
            if (object != null) {
                addInference(model, obligations, subject, RDFS.RANGE, object);
            }
        }
    }

    private static void addDataPropertyHierarchy(
            Reasoner reasoner,
            OWLDataProperty property,
            OWLOntology ontology,
            Model model,
            Model obligations,
            Set<Value> sourceTerms)
            throws Exception {
        IRI subject = iriValue(property.getIRI().toString(), sourceTerms);
        if (subject == null) {
            return;
        }
        for (org.semanticweb.owlapi.model.OWLDataProperty parent
                : reasoner.getSuperDataProperties(property, false).getFlattened()) {
            IRI object = iriValue(parent.getIRI().toString(), sourceTerms);
            if (object != null) {
                addInference(model, obligations, subject, RDFS.SUBPROPERTYOF, object);
            }
        }
        for (org.semanticweb.owlapi.model.OWLDataProperty equivalent
                : reasoner.getEquivalentDataProperties(property).getEntities()) {
            IRI object = iriValue(equivalent.getIRI().toString(), sourceTerms);
            if (object != null && !object.equals(subject)) {
                addInference(model, obligations, subject, OWL.EQUIVALENTPROPERTY, object);
            }
        }
        for (OWLClass domain : reasoner.getDataPropertyDomains(property, false).getFlattened()) {
            IRI object = iriValue(domain.getIRI().toString(), sourceTerms);
            if (object != null) {
                addInference(model, obligations, subject, RDFS.DOMAIN, object);
            }
        }
        Set<OWLDataProperty> rangeProperties = new HashSet<>();
        rangeProperties.add(property);
        rangeProperties.addAll(reasoner.getSuperDataProperties(property, false).getFlattened());
        for (OWLDataProperty rangeProperty : rangeProperties) {
            for (OWLDataRange range : rangeProperty.getRanges(ontology)) {
                if (!range.isDatatype()) {
                    throw new NotAvailableException(
                            "anonymous data-property range cannot be mapped into the finite source domain");
                }
                IRI object = iriValue(range.asOWLDatatype().getIRI().toString(), sourceTerms);
                if (object == null) {
                    throw new NotAvailableException(
                            "data-property range uses a datatype outside the finite source domain");
                }
                addInference(model, obligations, subject, RDFS.RANGE, object);
            }
        }
    }

    private static Resource iriResource(String value, Set<Value> sourceTerms) {
        IRI iri = iriValue(value, sourceTerms);
        return iri;
    }

    private static void addInference(
            Model model, Model obligations, Resource subject, IRI predicate, Value object)
            throws NotAvailableException {
        if (subject == null || predicate == null || object == null) {
            throw new NotAvailableException(
                    "inference term is outside the finite source domain");
        }
        if (model.add(subject, predicate, object)) {
            obligations.add(subject, predicate, object);
        }
    }

    private static Resource anonymousResource(
            OWLAnonymousIndividual individual, Set<Value> sourceTerms) {
        String identifier = individual.getID().getID();
        for (Value term : sourceTerms) {
            if (!(term instanceof BNode)) {
                continue;
            }
            String sourceIdentifier = ((BNode) term).getID();
            if (identifier.equals(sourceIdentifier)
                    || identifier.equals("_:" + sourceIdentifier)
                    || identifier.endsWith("-" + sourceIdentifier)) {
                return (Resource) term;
            }
        }
        return null;
    }

    private static IRI iriValue(String value, Set<Value> sourceTerms) {
        IRI iri = SimpleValueFactory.getInstance().createIRI(value);
        return sourceTerms.contains(iri) ? iri : null;
    }

    private static Literal rdfLiteral(org.semanticweb.owlapi.model.OWLLiteral literal) {
        SimpleValueFactory values = SimpleValueFactory.getInstance();
        if (literal.hasLang()) {
            return values.createLiteral(literal.getLiteral(), literal.getLang());
        }
        return values.createLiteral(
                literal.getLiteral(),
                values.createIRI(literal.getDatatype().getIRI().toString()));
    }

    private static org.semanticweb.owlapi.model.OWLLiteral owlLiteral(
            OWLDataFactory factory, Literal literal) {
        if (literal.getLanguage().isPresent()) {
            return factory.getOWLLiteral(literal.getLabel(), literal.getLanguage().get());
        }
        return factory.getOWLLiteral(
                literal.getLabel(),
                factory.getOWLDatatype(
                        org.semanticweb.owlapi.model.IRI.create(
                                literal.getDatatype().stringValue())));
    }

}
