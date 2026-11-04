// PortCode helper for Ghidra headless. Args: <out-file> list [max] | func <name-or-address> | strings [max]
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.Address;
import ghidra.program.model.data.StringDataInstance;
import java.nio.file.*;

public class PcDecompile extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        if (a.length < 2) { Files.writeString(Paths.get(a[0]), "usage: <out> list|func|strings ..."); return; }
        StringBuilder sb = new StringBuilder();
        String mode = a[1];
        int max = a.length > 2 && a[1].equals("list") || a.length > 2 && a[1].equals("strings") ? Integer.parseInt(a[2]) : 200;
        if (mode.equals("list")) {
            int n = 0;
            for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
                if (n++ >= max) { sb.append("... more functions omitted\n"); break; }
                sb.append(f.getEntryPoint()).append("  ").append(f.getName()).append("  size=")
                  .append(f.getBody().getNumAddresses()).append("\n");
            }
        } else if (mode.equals("strings")) {
            int n = 0;
            for (Data d : currentProgram.getListing().getDefinedData(true)) {
                if (!d.hasStringValue()) continue;
                if (n++ >= max) { sb.append("... more strings omitted\n"); break; }
                sb.append(d.getAddress()).append("  ").append(d.getDefaultValueRepresentation()).append("\n");
            }
        } else if (mode.equals("func")) {
            String q = a[2];
            Function target = null;
            for (Function f : currentProgram.getFunctionManager().getFunctions(true))
                if (f.getName().equals(q)) { target = f; break; }
            if (target == null) {
                try { Address ad = toAddr(q); if (ad != null) target = currentProgram.getFunctionManager().getFunctionContaining(ad); } catch (Exception e) {}
            }
            if (target == null) { sb.append("no function named or containing '").append(q).append("'. Use mode=list.\n"); }
            else {
                DecompInterface di = new DecompInterface();
                di.openProgram(currentProgram);
                DecompileResults r = di.decompileFunction(target, 60, monitor);
                if (r.decompileCompleted()) sb.append(r.getDecompiledFunction().getC());
                else sb.append("decompile failed: ").append(r.getErrorMessage());
            }
        } else sb.append("unknown mode ").append(mode);
        Files.writeString(Paths.get(a[0]), sb.toString());
    }
}
