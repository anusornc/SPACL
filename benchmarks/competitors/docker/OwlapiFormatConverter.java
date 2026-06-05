import java.io.File;
import java.util.Locale;
import org.semanticweb.owlapi.apibinding.OWLManager;
import org.semanticweb.owlapi.formats.FunctionalSyntaxDocumentFormat;
import org.semanticweb.owlapi.formats.OWLXMLDocumentFormat;
import org.semanticweb.owlapi.model.IRI;
import org.semanticweb.owlapi.model.OWLOntology;
import org.semanticweb.owlapi.model.OWLOntologyManager;
import org.semanticweb.owlapi.util.OWLOntologyMerger;

public final class OwlapiFormatConverter {
    private OwlapiFormatConverter() {}

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("Usage: OwlapiFormatConverter <input> <output> <owlxml|functional> [merge]");
            System.exit(2);
        }

        File input = new File(args[0]);
        File output = new File(args[1]);
        String formatKey = args[2].toLowerCase(Locale.ROOT);
        boolean mergeImports = args.length > 3 && "merge".equalsIgnoreCase(args[3]);

        OWLOntologyManager manager = OWLManager.createOWLOntologyManager();
        OWLOntology ontology = manager.loadOntologyFromOntologyDocument(input);
        if (mergeImports) {
            ontology = new OWLOntologyMerger(manager).createMergedOntology(manager, IRI.generateDocumentIRI());
        }

        switch (formatKey) {
            case "owlxml":
                manager.saveOntology(ontology, new OWLXMLDocumentFormat(), IRI.create(output));
                break;
            case "functional":
                manager.saveOntology(ontology, new FunctionalSyntaxDocumentFormat(), IRI.create(output));
                break;
            default:
                throw new IllegalArgumentException("Unsupported format: " + formatKey);
        }
    }
}
