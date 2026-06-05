import java.io.File;
import java.util.Locale;
import org.semanticweb.owlapi.apibinding.OWLManager;
import org.semanticweb.owlapi.model.OWLOntology;
import org.semanticweb.owlapi.model.OWLOntologyManager;
import org.semanticweb.owlapi.reasoner.OWLReasoner;
import org.semanticweb.owlapi.reasoner.OWLReasonerFactory;

public final class OwlapiConsistencyRunner {
    private OwlapiConsistencyRunner() {}

    private static String[] factoryClassesFor(String reasonerKey) {
        switch (reasonerKey) {
            case "openllet":
                return new String[] {"openllet.owlapi.OpenlletReasonerFactory"};
            case "elk":
                return new String[] {"org.semanticweb.elk.owlapi.ElkReasonerFactory"};
            case "jfact":
                return new String[] {"uk.ac.manchester.cs.jfact.JFactFactory"};
            case "pellet":
                return new String[] {
                    "com.clarkparsia.pellet.owlapiv3.PelletReasonerFactory",
                    "org.mindswap.pellet.owlapi.ReasonerFactory"
                };
            default:
                return null;
        }
    }

    private static String jsonEscape(String raw) {
        String s = raw == null ? "" : raw;
        return s
                .replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", " ")
                .replace("\r", " ");
    }

    private static String throwableSummary(Throwable t) {
        if (t == null) {
            return "Unknown error";
        }
        StringBuilder sb = new StringBuilder();
        sb.append(t.getClass().getSimpleName());
        if (t.getMessage() != null && !t.getMessage().isEmpty()) {
            sb.append(": ").append(t.getMessage());
        }
        Throwable cause = t.getCause();
        int depth = 0;
        while (cause != null && depth < 3) {
            sb.append(" | cause: ").append(cause.getClass().getSimpleName());
            if (cause.getMessage() != null && !cause.getMessage().isEmpty()) {
                sb.append(": ").append(cause.getMessage());
            }
            cause = cause.getCause();
            depth++;
        }
        return sb.toString();
    }

    private static OWLReasonerFactory resolveFactory(String reasonerKey) throws Exception {
        String[] factoryClassNames = factoryClassesFor(reasonerKey);
        if (factoryClassNames == null) {
            throw new IllegalArgumentException("Unsupported reasoner: " + reasonerKey);
        }

        ClassNotFoundException lastClassNotFound = null;
        for (String factoryClassName : factoryClassNames) {
            try {
                Class<?> factoryClass = Class.forName(factoryClassName);
                try {
                    Object singleton = factoryClass.getMethod("getInstance").invoke(null);
                    return (OWLReasonerFactory) singleton;
                } catch (NoSuchMethodException ignored) {
                    return (OWLReasonerFactory) factoryClass.getDeclaredConstructor().newInstance();
                }
            } catch (ClassNotFoundException cnfe) {
                lastClassNotFound = cnfe;
            }
        }
        if (lastClassNotFound != null) {
            throw lastClassNotFound;
        }
        throw new IllegalStateException("No factory class resolved for reasoner: " + reasonerKey);
    }

    public static void main(String[] args) {
        if (args.length < 2) {
            System.err.println("Usage: OwlapiConsistencyRunner <openllet|elk|jfact|pellet> <ontology.owl> [operation]");
            System.exit(2);
        }

        String reasonerKey = args[0].toLowerCase(Locale.ROOT);
        String ontologyFile = args[1];
        String operation = args.length > 2 ? args[2] : "consistency";

        if (!"consistency".equals(operation)) {
            System.out.println(
                    "{\"duration_ms\": -1, \"status\": \"unsupported_operation\", \"reasoning_result\": \"unknown\", "
                            + "\"message\": \"Only consistency is supported\"}");
            System.exit(2);
        }

        long started = System.nanoTime();
        try {
            OWLOntologyManager manager = OWLManager.createOWLOntologyManager();
            OWLOntology ontology = manager.loadOntologyFromOntologyDocument(new File(ontologyFile));

            OWLReasonerFactory factory = resolveFactory(reasonerKey);
            OWLReasoner reasoner = factory.createReasoner(ontology);

            long reasonerStart = System.nanoTime();
            boolean consistent = reasoner.isConsistent();
            long reasonerEnd = System.nanoTime();

            reasoner.dispose();

            long durationMs = (reasonerEnd - reasonerStart) / 1_000_000;
            String result = consistent ? "consistent" : "inconsistent";
            System.out.println(
                    "{\"duration_ms\": "
                            + durationMs
                            + ", \"status\": \"completed\", \"reasoning_result\": \""
                            + result
                            + "\"}");
        } catch (Throwable t) {
            long durationMs = (System.nanoTime() - started) / 1_000_000;
            String message = throwableSummary(t);
            t.printStackTrace(System.err);
            System.err.println(message);
            System.out.println(
                    "{\"duration_ms\": "
                            + durationMs
                            + ", \"status\": \"failed\", \"reasoning_result\": \"unknown\", \"error\": \""
                            + jsonEscape(message)
                            + "\"}");
            System.exit(1);
        }
    }
}
