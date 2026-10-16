// Optional interoperability probe. Compile against a locally installed Cubism
// Editor's res/*.jar; proprietary binaries are never copied into the repository.
package kasane.cmo3probe;
import java.io.File;
import java.util.HashMap;
import org.jdom.Document;
import org.jdom.ProcessingInstruction;
import org.jdom.input.SAXBuilder;
import com.live2d.serialize.XmlReader;
import com.live2d.serialize.archiver.ArchiveReader;
import com.live2d.type.CArrayList;
import com.live2d.cubism.doc.model.CModelSource;

public final class ReadCmo3 {
    public static void main(String[] args) {
        try { check(args); System.exit(0); }
        catch (Throwable error) { error.printStackTrace(); System.exit(1); }
    }
    private static void check(String[] args) throws Exception {
        ArchiveReader archive = new ArchiveReader(new File(args[0]));
        byte[] xml = archive.getFirstFileByTag("main_xml");
        Document doc = new SAXBuilder().build(new java.io.ByteArrayInputStream(xml));
        HashMap<String,Object> properties = new HashMap<>();
        if (args[0].endsWith(".can3")) {
            // The local Editor's resource reader needs the same relocation
            // context that its animation loader supplies before deserialization.
            Class<?> resolverType = Class.forName("com.live2d.cubism.doc.animation.movie.res.a");
            Object resolver = resolverType.getConstructor().newInstance();
            resolverType.getMethod("c", File.class).invoke(resolver, new File(args[0]));
            properties.put("KEY_FILE_RESOLVER", resolver);
        }
        XmlReader reader = new XmlReader(archive, properties);
        reader.setFileNameForDebug(args[0]);
        CArrayList<String> names = new CArrayList<>();
        for (Object item : doc.getContent()) {
            if (item instanceof ProcessingInstruction) {
                reader.parse_ProcessingInstruction((ProcessingInstruction)item, names);
            }
        }
        Object root = reader.readRootObject(doc.getRootElement());
        java.lang.reflect.Field errors = com.live2d.serialize.XmlParam.class.getDeclaredField("errorList");
        errors.setAccessible(true);
        if (!((java.util.Map<?,?>)errors.get(reader)).isEmpty()) throw new AssertionError("Cubism reported serialization errors: " + errors.get(reader));
        if (root instanceof com.live2d.cubism.doc.animation.CAnimation) {
            checkAnimation((com.live2d.cubism.doc.animation.CAnimation)root, new File(args[0]));
            archive.close();
            return;
        }
        if (!(root instanceof CModelSource)) throw new AssertionError("Unknown Cubism root: " + root);
        CModelSource model = (CModelSource)root;
        System.out.println("Cubism read: " + model.getName());
        model.setup();
        System.out.println("Objects: " + model.getAllArtMeshes().size() + " meshes, "
            + model.getAllDeformers().size() + " deformers, " + model.getAllParameters().size()
            + " parameters, " + model.getAllGlues().size() + " glues");
        com.live2d.cubism.doc.model.CModel instance = model.createModelInstance(com.live2d.graphics3d.rendering.RenderType.NORMAL);
        model.updateModelInstances();
        for (com.live2d.cubism.doc.model.drawable.artMesh.CArtMesh mesh : instance.getAllArtMeshes()) {
            float[] positions = mesh.getPositions();
            for (float value : positions) if (!Float.isFinite(value)) throw new AssertionError("Non-finite mesh position");
            for (int index : mesh.getIndices()) if (index < 0 || index >= positions.length / 2) throw new AssertionError("Invalid triangle index");
        }
        if (args.length > 1) checkGeometry(model, instance, new File(args[1]), Double.parseDouble(args[2]));
        archive.close();
    }
    private static void checkGeometry(CModelSource model, com.live2d.cubism.doc.model.CModel instance,
                                      File reference, double tolerance) throws Exception {
        java.util.List<?> cases;
        try (java.io.Reader input = new java.io.FileReader(reference, java.nio.charset.StandardCharsets.UTF_8)) {
            cases = (java.util.List<?>)new org.json.simple.parser.JSONParser().parse(input);
        }
        if (cases.isEmpty()) throw new AssertionError("No reference poses");
        double maximum = 0;
        String worst = "";
        int vertices = 0;
        for (Object raw : cases) {
            java.util.Map<?,?> row = (java.util.Map<?,?>)raw;
            java.util.Map<?,?> parameters = (java.util.Map<?,?>)row.get("parameters");
            java.util.Map<?,?> positions = (java.util.Map<?,?>)row.get("positions");
            instance.getParameterSet().setDefaultValues();
            for (java.util.Map.Entry<?,?> entry : parameters.entrySet()) {
                com.live2d.cubism.doc.model.id.CParameterId id = new com.live2d.cubism.doc.model.id.CParameterId((String)entry.getKey());
                if (instance.getParameterSet().get(id) == null) throw new AssertionError("Missing parameter: " + id);
                instance.getParameterSet().setValue(id, ((Number)entry.getValue()).floatValue());
            }
            model.updateModelInstances();
            if (positions.size() != instance.getAllArtMeshes().size()) throw new AssertionError("Mesh count differs from reference");
            for (com.live2d.cubism.doc.model.drawable.artMesh.CArtMesh mesh : instance.getAllArtMeshes()) {
                String id = mesh.getId().toString();
                java.util.List<?> expected = (java.util.List<?>)positions.get(id);
                float[] actual = mesh.getPositions();
                if (expected == null || expected.size() * 2 != actual.length) throw new AssertionError("Geometry missing or resized: " + id);
                for (int i=0; i<expected.size(); i++) {
                    java.util.List<?> point = (java.util.List<?>)expected.get(i);
                    double dx = actual[i*2] - ((Number)point.get(0)).doubleValue();
                    double dy = actual[i*2+1] - ((Number)point.get(1)).doubleValue();
                    double error = Math.hypot(dx,dy);
                    if (!Double.isFinite(error)) throw new AssertionError("Non-finite geometry: " + id);
                    vertices++;
                    if (error > maximum) { maximum=error; worst=id + " vertex " + i + " parameters " + parameters; }
                }
            }
        }
        System.out.println("Geometry: " + cases.size() + " poses, " + vertices + " vertices, max error " + maximum + " px");
        if (maximum > tolerance) throw new AssertionError("Geometry error exceeds " + tolerance + " px: " + worst);
    }
    private static void checkAnimation(com.live2d.cubism.doc.animation.CAnimation animation, File file) throws Exception {
        animation.setFile(file);
        animation.setup();
        if (animation.getScenes().isEmpty()) throw new AssertionError("No editable scenes");
        System.out.println("Cubism read: animation " + animation.getName());
        for (com.live2d.cubism.doc.animation.CSceneSource scene : animation.getScenes()) {
            int duration = scene.getMovieInfo().getDuration();
            double fps = scene.getMovieInfo().getFps();
            if (duration < 1 || !Double.isFinite(fps) || fps <= 0) throw new AssertionError("Invalid timeline");
            for (com.live2d.cubism.doc.animation.movie.track.ICMvTrack_Source track : scene.getAllTracks()) {
                if (!(track instanceof com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source)) continue;
                com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source model =
                    (com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source)track;
                File linked = model.getResourceRef().getSrcFile();
                if (linked == null) throw new AssertionError("Missing CMO3 link");
                if (!linked.isAbsolute()) linked = new File(file.getParentFile(), linked.getPath());
                if (!linked.isFile()) throw new AssertionError("Linked model does not exist: " + linked);
                for (com.live2d.cubism.doc.animation.movie.effect.ICMvEffect effect : model.getEffectManager().getEffectList()) {
                    for (com.live2d.cubism.doc.animation.movie.effect.attr.ICMvAttr attr : effect.getAttrList()) {
                        if (!(attr instanceof com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF)) continue;
                        com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF value =
                            (com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF)attr;
                        for (double frame : new double[]{0, duration / 2.0, duration}) {
                            double sample = value.getValueData().getDoubleValue(frame);
                            if (!Double.isFinite(sample)) throw new AssertionError("Invalid curve: " + attr.getName());
                        }
                    }
                }
            }
            System.out.println("Objects: scene " + scene.getSceneName() + ", " + duration + " frames at " + fps + " FPS");
        }
    }

}
