// RustportExport.java -- local-only bounded Ghidra headless metadata exporter.
// Usage: -postScript RustportExport.java <json_output> <max_functions> <max_edges_per_function> <decompile_limit>
// Reads the current analysis Program, never starts the imported executable.
// Optional decompilation produces copyrighted/proprietary pseudocode: keep output local.
// Tested through command-line contract fixtures; requires installed Ghidra to execute.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.symbol.Reference;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

public class RustportExport extends GhidraScript {
    private int bounded(String s, int cap) {
        int n = Integer.parseInt(s);
        if (n < 0 || n > cap) throw new IllegalArgumentException("Limit outside [0," + cap + "]: " + s);
        return n;
    }

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 4) throw new IllegalArgumentException("output max_functions max_edges_per_function decompile_limit required");
        int maxFunctions = bounded(args[1], 100000);
        int maxEdges = bounded(args[2], 10000);
        int maxDecompile = bounded(args[3], 500);
        if (maxFunctions == 0) throw new IllegalArgumentException("No functions requested");
        JsonObject root = new JsonObject();
        root.addProperty("schema", 1);
        root.addProperty("tool", "ghidra-headless");
        root.addProperty("program", currentProgram.getName());
        root.addProperty("language", currentProgram.getLanguage().getLanguageID().getIdAsString());
        root.addProperty("image_base", currentProgram.getImageBase().toString());
        root.addProperty("executable_format", currentProgram.getExecutableFormat());
        JsonArray functions = new JsonArray();
        FunctionIterator iter = currentProgram.getFunctionManager().getFunctions(true);
        DecompInterface decomp = null;
        try {
            if (maxDecompile > 0) {
                decomp = new DecompInterface();
                if (!decomp.openProgram(currentProgram)) throw new IllegalStateException("Ghidra decompiler failed to initialize");
            }
            int count = 0;
            while (iter.hasNext() && count < maxFunctions) {
                if (monitor.isCancelled()) throw new InterruptedException("Cancelled");
                Function fn = iter.next();
                JsonObject row = new JsonObject();
                row.addProperty("name", fn.getName());
                row.addProperty("entry", fn.getEntryPoint().toString());
                row.addProperty("body_addresses", fn.getBody().getNumAddresses());
                row.addProperty("external", fn.isExternal());
                row.addProperty("thunk", fn.isThunk());
                JsonArray calls = new JsonArray();
                InstructionIterator instructions = currentProgram.getListing().getInstructions(fn.getBody(), true);
                int edges = 0;
                while (instructions.hasNext()) {
                    Instruction ins = instructions.next();
                    for (Reference ref : ins.getReferencesFrom()) {
                        if (ref.getReferenceType().isCall()) {
                            if (edges < maxEdges) {
                                JsonObject edge = new JsonObject();
                                edge.addProperty("from", ins.getAddress().toString());
                                edge.addProperty("to", ref.getToAddress().toString());
                                calls.add(edge);
                            }
                            edges++;
                        }
                    }
                }
                row.add("calls", calls);
                row.addProperty("call_edges_found", edges);
                row.addProperty("call_edges_truncated", edges > maxEdges);
                if (decomp != null && count < maxDecompile && !fn.isExternal()) {
                    DecompileResults result = decomp.decompileFunction(fn, 20, monitor);
                    if (result != null && result.decompileCompleted() && result.getDecompiledFunction() != null) {
                        String pseudo = result.getDecompiledFunction().getC();
                        row.addProperty("pseudocode", pseudo.length() <= 32000 ? pseudo : pseudo.substring(0, 32000));
                        row.addProperty("pseudocode_truncated", pseudo.length() > 32000);
                    } else {
                        row.addProperty("decompile_status", "not completed");
                    }
                }
                functions.add(row);
                count++;
            }
            root.add("functions", functions);
            root.addProperty("functions_exported", count);
            root.addProperty("functions_truncated", iter.hasNext());
            Path output = Path.of(args[0]);
            Path parent = output.toAbsolutePath().getParent();
            if (parent != null) Files.createDirectories(parent);
            Files.writeString(output, new GsonBuilder().disableHtmlEscaping().create().toJson(root), StandardCharsets.UTF_8);
            println("RUSTPORT_EXPORT_OK functions=" + count + " output=" + output);
        } finally {
            if (decomp != null) decomp.dispose();
        }
    }
}
