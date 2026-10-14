import org.semanticweb.owlapi.apibinding.OWLManager;
import org.semanticweb.owlapi.model.*;
import org.semanticweb.owlapi.reasoner.*;
import openllet.owlapi.OpenlletReasonerFactory;
import java.io.File;

public class PelletConsistency {
    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("Usage: PelletConsistency <ontology.owl>");
            System.exit(1);
        }
        
        String ontologyFile = args[0];
        
        // Load ontology
        OWLOntologyManager manager = OWLManager.createOWLOntologyManager();
        OWLOntology ontology = manager.loadOntologyFromOntologyDocument(new File(ontologyFile));
        
        // Create Pellet/Openllet reasoner
        OWLReasonerFactory reasonerFactory = OpenlletReasonerFactory.getInstance();
        OWLReasoner reasoner = reasonerFactory.createReasoner(ontology);
        
        // Check consistency
        long startTime = System.currentTimeMillis();
        boolean consistent = reasoner.isConsistent();
        long endTime = System.currentTimeMillis();
        
        System.out.println("Consistency: " + (consistent ? "CONSISTENT" : "INCONSISTENT"));
        System.out.println("Duration: " + (endTime - startTime) + " ms");
        
        reasoner.dispose();
        System.exit(consistent ? 0 : 1);
    }
}
