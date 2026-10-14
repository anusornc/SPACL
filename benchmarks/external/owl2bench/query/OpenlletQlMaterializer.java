import java.io.File;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashSet;
import java.util.Set;
import java.util.TreeSet;
import java.util.stream.Collectors;

import openllet.owlapi.OpenlletReasonerFactory;
import org.semanticweb.owlapi.apibinding.OWLManager;
import org.semanticweb.owlapi.model.IRI;
import org.semanticweb.owlapi.model.OWLClass;
import org.semanticweb.owlapi.model.OWLDataProperty;
import org.semanticweb.owlapi.model.OWLLiteral;
import org.semanticweb.owlapi.model.OWLNamedIndividual;
import org.semanticweb.owlapi.model.OWLObjectProperty;
import org.semanticweb.owlapi.model.OWLObjectPropertyExpression;
import org.semanticweb.owlapi.model.OWLOntology;
import org.semanticweb.owlapi.model.OWLOntologyManager;
import org.semanticweb.owlapi.reasoner.OWLReasoner;

public final class OpenlletQlMaterializer {
    private static final String RDF_TYPE =
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    private static final String RDFS_SUBCLASS =
            "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    private static final String RDFS_SUBPROPERTY =
            "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

    private OpenlletQlMaterializer() {}

    public static void main(String[] args) throws Exception {
        if (args.length < 1 || args.length > 2) {
            throw new IllegalArgumentException(
                    "usage: OpenlletQlMaterializer <source.owl-or-nq> [query-iris.txt]");
        }

        Set<IRI> queryIris = new HashSet<>();
        if (args.length == 2) {
            for (String line : Files.readAllLines(Path.of(args[1]))) {
                String value = line.strip();
                if (!value.isEmpty() && !value.startsWith("#")) {
                    queryIris.add(IRI.create(value));
                }
            }
        }

        OWLOntologyManager manager = OWLManager.createOWLOntologyManager();
        OWLOntology ontology = manager.loadOntologyFromOntologyDocument(new File(args[0]));
        OWLReasoner reasoner = OpenlletReasonerFactory.getInstance().createReasoner(ontology);
        try {
            if (!reasoner.isConsistent()) {
                throw new IllegalStateException("Openllet QL source ontology is inconsistent");
            }

            Set<OWLClass> classes = ontology.classesInSignature().collect(Collectors.toSet());
            Set<OWLObjectProperty> objectProperties =
                    ontology.objectPropertiesInSignature().collect(Collectors.toSet());
            Set<OWLDataProperty> dataProperties =
                    ontology.dataPropertiesInSignature().collect(Collectors.toSet());
            Set<OWLNamedIndividual> individuals =
                    ontology.individualsInSignature().collect(Collectors.toSet());
            TreeSet<String> output = new TreeSet<>();

            for (OWLClass sourceClass : classes) {
                for (OWLClass target : reasoner.getSuperClasses(sourceClass, false).getFlattened()) {
                    if (classes.contains(target)
                            && wantedEither(sourceClass.getIRI(), target.getIRI(), queryIris)) {
                        output.add(iri(sourceClass.getIRI()) + " " + iri(IRI.create(RDFS_SUBCLASS))
                                + " " + iri(target.getIRI()) + " .");
                    }
                }
            }

            for (OWLObjectProperty sourceProperty : objectProperties) {
                for (OWLObjectPropertyExpression targetExpression :
                        reasoner.getSuperObjectProperties(sourceProperty, false).getFlattened()) {
                    if (!targetExpression.isAnonymous()) {
                        OWLObjectProperty target = targetExpression.asOWLObjectProperty();
                        if (objectProperties.contains(target)
                                && wantedEither(sourceProperty.getIRI(), target.getIRI(), queryIris)) {
                            output.add(iri(sourceProperty.getIRI()) + " "
                                    + iri(IRI.create(RDFS_SUBPROPERTY)) + " "
                                    + iri(target.getIRI()) + " .");
                        }
                    }
                }
            }

            for (OWLDataProperty sourceProperty : dataProperties) {
                for (OWLDataProperty target :
                        reasoner.getSuperDataProperties(sourceProperty, false).getFlattened()) {
                    if (dataProperties.contains(target)
                            && wantedEither(sourceProperty.getIRI(), target.getIRI(), queryIris)) {
                        output.add(iri(sourceProperty.getIRI()) + " "
                                + iri(IRI.create(RDFS_SUBPROPERTY)) + " "
                                + iri(target.getIRI()) + " .");
                    }
                }
            }

            for (OWLNamedIndividual subject : individuals) {
                for (OWLClass type : reasoner.getTypes(subject, false).getFlattened()) {
                    if (classes.contains(type) && wanted(type.getIRI(), queryIris)) {
                        output.add(iri(subject.getIRI()) + " " + iri(IRI.create(RDF_TYPE))
                                + " " + iri(type.getIRI()) + " .");
                    }
                }
                for (OWLObjectProperty property : objectProperties) {
                    if (!wanted(property.getIRI(), queryIris)) {
                        continue;
                    }
                    for (OWLNamedIndividual object :
                            reasoner.getObjectPropertyValues(subject, property).getFlattened()) {
                        if (individuals.contains(object)) {
                            output.add(iri(subject.getIRI()) + " " + iri(property.getIRI())
                                    + " " + iri(object.getIRI()) + " .");
                        }
                    }
                }
                for (OWLDataProperty property : dataProperties) {
                    if (!wanted(property.getIRI(), queryIris)) {
                        continue;
                    }
                    for (OWLLiteral value : reasoner.getDataPropertyValues(subject, property)) {
                        output.add(iri(subject.getIRI()) + " " + iri(property.getIRI())
                                + " " + literal(value) + " .");
                    }
                }
            }

            for (String line : output) {
                System.out.println(line);
            }
        } finally {
            reasoner.dispose();
            manager.removeOntology(ontology);
        }
    }

    private static String iri(IRI value) {
        return "<" + value.toString() + ">";
    }

    private static boolean wanted(IRI value, Set<IRI> queryIris) {
        return queryIris.isEmpty() || queryIris.contains(value);
    }

    private static boolean wantedEither(IRI left, IRI right, Set<IRI> queryIris) {
        return queryIris.isEmpty() || queryIris.contains(left) || queryIris.contains(right);
    }

    private static String literal(OWLLiteral value) {
        String lexical = value.getLiteral()
                .replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\n")
                .replace("\r", "\\r");
        if (value.hasLang()) {
            return "\"" + lexical + "\"@" + value.getLang();
        }
        return "\"" + lexical + "\"^^" + iri(value.getDatatype().getIRI());
    }
}
