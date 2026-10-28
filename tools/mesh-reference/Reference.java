// SPDX-License-Identifier: GPL-3.0-only
// Run against a locally compiled PSD2Live checkout; no application UI is opened.
import io.github.psd2live.core.*;
import java.nio.file.*;
import java.util.*;
public class Reference {
  static boolean painted(String name, double x, double y) {
    return switch(name) {
      case "ellipse" -> Math.pow((x-96)/78,2)+Math.pow((y-80)/62,2)<1;
      case "hole" -> Math.hypot(x-96,y-80)<62 && Math.hypot(x-96,y-80)>22;
      case "ribbon" -> x>10 && x<180 && y>78 && y<81;
      case "concave" -> x>20 && x<170 && y>20 && y<140 && !(x>65 && x<125 && y<95);
      default -> false;
    };
  }
  public static void main(String[] args) throws Exception {
    StringBuilder out=new StringBuilder("{\"source_revision\":\""+args[1]+"\",\"cases\":["); boolean first=true;
    for(String name:List.of("ellipse","hole","ribbon","concave")) {
      int w=192,h=160; byte[] rgba=new byte[w*h*4];
      for(int y=0;y<h;y++)for(int x=0;x<w;x++)if(painted(name,x+.5,y+.5))rgba[(y*w+x)*4+3]=(byte)255;
      for(MeshEdgeMode edge:MeshEdgeMode.values())for(MeshFillAlgorithm fill:MeshFillAlgorithm.values()) {
        MeshSettings settings=new MeshSettings(1,edge,8,12,28,fill,false,new MeshFillParameters(),0);
        long start=System.nanoTime();var mesh=AdaptiveMeshGenerator.INSTANCE.generate(w,h,rgba,8,settings,1,1);long elapsed=System.nanoTime()-start;
        if(!first)out.append(',');first=false;
        out.append("{\"name\":\"").append(name).append("\",\"edge\":\"").append(edge).append("\",\"fill\":\"").append(fill).append("\",\"width\":192,\"height\":160,\"elapsed_ns\":").append(elapsed);
        if(mesh==null){out.append(",\"failed\":true}");continue;}
        out.append(",\"positions\":").append(Arrays.toString(mesh.getPositions()));
        out.append(",\"indices\":").append(Arrays.toString(mesh.getIndices()));
        out.append(",\"boundary_loops\":[");boolean sep=false;for(int[] loop:mesh.getBoundaryLoops()){if(sep)out.append(',');sep=true;out.append(Arrays.toString(loop));}out.append("]}");
      }
    }
    out.append("]}");Files.writeString(Path.of(args[0]),out.toString());
  }
}
