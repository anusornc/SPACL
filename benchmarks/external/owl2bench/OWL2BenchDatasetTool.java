import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;

import org.eclipse.rdf4j.model.Model;
import org.eclipse.rdf4j.model.impl.LinkedHashModel;
import org.eclipse.rdf4j.rio.RDFFormat;
import org.eclipse.rdf4j.rio.RDFParser;
import org.eclipse.rdf4j.rio.RDFWriter;
import org.eclipse.rdf4j.rio.helpers.StatementCollector;
import org.eclipse.rdf4j.rio.nquads.NQuadsWriter;
import org.eclipse.rdf4j.rio.rdfxml.RDFXMLParser;
import org.semanticweb.owlapi.apibinding.OWLManager;
import org.semanticweb.owlapi.formats.RDFXMLDocumentFormat;
import org.semanticweb.owlapi.model.IRI;
import org.semanticweb.owlapi.model.OWLAxiom;
import org.semanticweb.owlapi.model.OWLOntology;
import org.semanticweb.owlapi.model.OWLOntologyManager;
import org.semanticweb.owlapi.model.parameters.Imports;

/**
 * Small, version-pinned bridge used by the OWL2Bench dataset freezer.
 *
 * <p>The bridge deliberately uses the OWL API and RDF4J bundled in the
 * official OWL2Bench generator artifact. It only converts RDF/XML to N-Quads
 * and extracts explicit ABox axioms; it does not reason or produce query
 * answers.</p>
 */
public final class OWL2BenchDatasetTool {
    private OWL2BenchDatasetTool() {}

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 3) {
            throw new IllegalArgumentException(
                    "usage: rdfxml-to-nquads <input.rdf> <output.nq>"
                            + " | extract-abox <input.rdf> <output.rdf>");
        }
        Path input = Path.of(arguments[1]).toAbsolutePath().normalize();
        Path output = Path.of(arguments[2]).toAbsolutePath().normalize();
        Files.createDirectories(output.getParent());
        switch (arguments[0]) {
            case "rdfxml-to-nquads":
                writeNQuads(parseRdfXml(input), output);
                break;
            case "extract-abox":
                extractABox(input, output);
                break;
            default:
                throw new IllegalArgumentException("unknown command: " + arguments[0]);
        }
    }

    private static Model parseRdfXml(Path input) throws Exception {
        Model model = new LinkedHashModel();
        RDFParser parser = new RDFXMLParser();
        parser.setPreserveBNodeIDs(true);
        parser.setRDFHandler(new StatementCollector(model));
        try (InputStream stream = Files.newInputStream(input)) {
            parser.parse(stream, input.toUri().toString());
        }
        return model;
    }

    private static void writeNQuads(Model model, Path output) throws Exception {
        try (OutputStream stream = Files.newOutputStream(output)) {
            RDFWriter writer = new NQuadsWriter(stream);
            writer.startRDF();
            for (org.eclipse.rdf4j.model.Statement statement : model) {
                writer.handleStatement(statement);
            }
            writer.endRDF();
        }
    }

    private static void extractABox(Path input, Path output) throws Exception {
        OWLOntologyManager manager = OWLManager.createOWLOntologyManager();
        OWLOntology source = manager.loadOntologyFromOntologyDocument(input.toFile());
        OWLOntology abox = manager.createOntology(
                source.getABoxAxioms(Imports.EXCLUDED),
                IRI.create("urn:owl2bench:extracted-abox"));
        try {
            manager.saveOntology(abox, new RDFXMLDocumentFormat(), IRI.create(output.toFile()));
        } finally {
            manager.removeOntology(abox);
            manager.removeOntology(source);
        }
    }
}
