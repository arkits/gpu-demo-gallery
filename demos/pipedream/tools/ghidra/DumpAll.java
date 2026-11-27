import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import java.io.*;
public class DumpAll extends GhidraScript {
 public void run() throws Exception {
  PrintWriter w=new PrintWriter(new FileWriter(getScriptArgs()[0]));
  DecompInterface d=new DecompInterface(); d.openProgram(currentProgram); int n=0;
  for(Function f: currentProgram.getFunctionManager().getFunctions(true)){
   if(f.getEntryPoint().getOffset()>=0x470000L) break;
   DecompileResults r=d.decompileFunction(f,60,monitor); w.println("//==== "+f.getName()+" @"+f.getEntryPoint()); if(r.getDecompiledFunction()!=null) w.println(r.getDecompiledFunction().getC()); n++; }
  w.close(); println("DUMPED "+n); }}
