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
    private static java.util.Map<String,String> check(String[] args) throws Exception {
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
        // Do not retain the entire input DOM while the native serializer builds
        // another DOM. Large, baked CAN3 files otherwise need several copies.
        doc = null;
        xml = null;
        reader = null;
        System.gc();
        if (root instanceof com.live2d.cubism.doc.animation.CAnimation) {
            com.live2d.cubism.doc.animation.CAnimation animation = (com.live2d.cubism.doc.animation.CAnimation)root;
            checkAnimation(animation, new File(args[0]));
            int next = 1;
            if (args.length > next && !args[next].equals("--resave")) {
                checkAnimationValues(animation, new File(args[next++]));
            }
            java.util.Map<String,String> operations = expressionOperations(animation);
            if (args.length > next && args[next].equals("--resave")) {
                File saved = new File(args[next + 1]);
                if (saved.exists()) throw new AssertionError("Refusing to overwrite native save destination");
                if (!animation.saveAnimation(saved, false)) throw new AssertionError("Cubism save failed");
                archive.close();
                archive = null;
                animation = null;
                root = null;
                System.gc();
                java.util.Map<String,String> reloaded = check(next > 1
                    ? new String[]{saved.getAbsolutePath(), args[1]}
                    : new String[]{saved.getAbsolutePath()});
                if (!operations.equals(reloaded)) throw new AssertionError("Expression operations changed during Cubism save/reload");
                System.out.println("Expression operations: " + operations.size() + " preserved through native save/reload");
            }
            if (archive != null) archive.close();
            return operations;
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
        return java.util.Collections.emptyMap();
    }
    private static java.util.Map<String,String> expressionOperations(com.live2d.cubism.doc.animation.CAnimation animation) {
        java.util.Map<String,String> result = new HashMap<>();
        for (com.live2d.cubism.doc.animation.CSceneSource scene : animation.getScenes()) {
            for (com.live2d.cubism.doc.animation.movie.track.ICMvTrack_Source track : scene.getAllTracks()) {
                if (!(track instanceof com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source)) continue;
                com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source model =
                    (com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source)track;
                for (com.live2d.cubism.doc.animation.movie.effect.ICMvEffect effect : model.getEffectManager().getEffectList()) {
                    for (com.live2d.cubism.doc.animation.movie.effect.attr.ICMvAttr attr : effect.getAttrList()) {
                        Object operation = attr.getOptionParam("KASANE_EXPRESSION_ENTRY_V1");
                        if (operation != null) result.put(scene.getSceneName() + "/" + attr.getId().getIdString(), operation.toString());
                    }
                }
            }
        }
        return result;
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
        int orderedPoses = 0;
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
            if (row.containsKey("render_order")) {
                java.util.List<com.live2d.cubism.doc.model.drawable.artMesh.CArtMesh> meshes =
                    new java.util.ArrayList<>(instance.getAllArtMeshes());
                meshes.sort(java.util.Comparator.comparingInt(mesh -> mesh.getRelativeDrawOrder()));
                java.util.List<String> actualOrder = new java.util.ArrayList<>();
                for (com.live2d.cubism.doc.model.drawable.artMesh.CArtMesh mesh : meshes) actualOrder.add(mesh.getId().toString());
                java.util.List<?> expectedOrder = (java.util.List<?>)row.get("render_order");
                if (!actualOrder.equals(expectedOrder)) {
                    int first = 0;
                    while (first < Math.min(actualOrder.size(), expectedOrder.size()) && actualOrder.get(first).equals(expectedOrder.get(first))) first++;
                    throw new AssertionError("Draw order differs at rank " + first + " parameters " + parameters
                        + ": expected " + (first < expectedOrder.size() ? expectedOrder.get(first) : "<end>")
                        + ", actual " + (first < actualOrder.size() ? actualOrder.get(first) : "<end>"));
                }
                orderedPoses++;
            }
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
        if (orderedPoses > 0) System.out.println("Draw order: " + orderedPoses + " poses match Kasane back-to-front");
        if (maximum > tolerance) throw new AssertionError("Geometry error exceeds " + tolerance + " px: " + worst);
    }
    private static void checkAnimationValues(com.live2d.cubism.doc.animation.CAnimation animation, File reference) throws Exception {
        java.util.List<?> cases;
        try (java.io.Reader input = new java.io.FileReader(reference, java.nio.charset.StandardCharsets.UTF_8)) {
            cases = (java.util.List<?>)new org.json.simple.parser.JSONParser().parse(input);
        }
        int samples = 0;
        double maximumError = 0;
        double maximumGeometryError = 0;
        double maximumRelativeError = 0;
        for (Object item : cases) {
            java.util.Map<?,?> expectedScene = (java.util.Map<?,?>)item;
            String name = (String)expectedScene.get("name");
            com.live2d.cubism.doc.animation.CSceneSource scene = null;
            for (com.live2d.cubism.doc.animation.CSceneSource candidate : animation.getScenes()) {
                if (candidate.getSceneName().equals(name)) scene = candidate;
            }
            if (scene == null) throw new AssertionError("Missing scene: " + name);
            if (expectedScene.containsKey("fps") && Math.abs(scene.getMovieInfo().getFps() - ((Number)expectedScene.get("fps")).doubleValue()) > 0.000001) {
                throw new AssertionError("Unexpected scene FPS: " + name + " " + scene.getMovieInfo().getFps());
            }
            HashMap<String,com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF> attributes = new HashMap<>();
            for (com.live2d.cubism.doc.animation.movie.track.ICMvTrack_Source track : scene.getAllTracks()) {
                if (!(track instanceof com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source)) continue;
                com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source model =
                    (com.live2d.cubism.doc.animation.movie.track.CMvTrack_Live2DModel_Source)track;
                for (com.live2d.cubism.doc.animation.movie.effect.ICMvEffect effect : model.getEffectManager().getEffectList()) {
                    for (com.live2d.cubism.doc.animation.movie.effect.attr.ICMvAttr attr : effect.getAttrList()) {
                        if (attr instanceof com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF) {
                            attributes.put(attr.getId().getIdString(), (com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF)attr);
                        }
                    }
                }
            }
            for (Object channel : (java.util.List<?>)expectedScene.get("tracks")) {
                java.util.Map<?,?> expected = (java.util.Map<?,?>)channel;
                String id = (String)expected.get("attribute");
                com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF attr = attributes.get(id);
                if (attr == null) throw new AssertionError("Missing channel: " + name + "/" + id);
                java.util.List<?> values = (java.util.List<?>)expected.get("values");
                for (int frame = 0; frame < values.size(); frame++) {
                    double actual = attr.getValueData().getDoubleValue(frame);
                    double value = ((Number)values.get(frame)).doubleValue();
                    double error = Math.abs(actual - value);
                    maximumError = Math.max(maximumError, error);
                    double[] geometry = exactSequenceValue(attr, frame);
                    maximumGeometryError = Math.max(maximumGeometryError, Math.abs(geometry[0] - value));
                    maximumRelativeError = Math.max(maximumRelativeError, Math.max(0, error - 0.0001) / Math.max(geometry[1], 0.0001));
                    if (!Double.isFinite(actual) || !Double.isFinite(geometry[0]) || !Double.isFinite(geometry[1]) || Math.abs(geometry[0] - value) > 0.0001
                        || error > 0.0001 + 0.005 * geometry[1]) {
                        throw new AssertionError(name + "/" + id + " frame " + frame + ": " + actual + " != " + value + curveDetails(attr,frame));
                    }
                    samples++;
                }
                if (expected.containsKey("checks")) {
                    for (Object check : (java.util.List<?>)expected.get("checks")) {
                        java.util.Map<?,?> row = (java.util.Map<?,?>)check;
                        double frame = ((Number)row.get("frame")).doubleValue();
                        double value = ((Number)row.get("value")).doubleValue();
                        double actual = attr.getValueData().getDoubleValue(frame);
                        double error = Math.abs(actual - value);
                        maximumError = Math.max(maximumError, error);
                        double[] geometry = exactSequenceValue(attr, frame);
                        maximumGeometryError = Math.max(maximumGeometryError, Math.abs(geometry[0] - value));
                        maximumRelativeError = Math.max(maximumRelativeError, Math.max(0, error - 0.0001) / Math.max(geometry[1], 0.0001));
                        if (!Double.isFinite(actual) || !Double.isFinite(geometry[0]) || !Double.isFinite(geometry[1]) || Math.abs(geometry[0] - value) > 0.0001
                            || error > 0.0001 + 0.005 * geometry[1]) {
                            throw new AssertionError(name + "/" + id + " subframe " + frame + ": " + actual + " != " + value);
                        }
                        samples++;
                    }
                }
            }
        }
        System.out.println("Animation: " + cases.size() + " scenes, " + samples
            + " samples, maximum serialized-curve error " + maximumGeometryError
            + ", maximum native-evaluator error " + maximumError
            + ", maximum excess error / segment range " + maximumRelativeError);
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

    private static double cubic(double a, double b, double c, double d, double t) {
        double s = 1 - t;
        return s*s*s*a + 3*s*s*t*b + 3*s*t*t*c + t*t*t*d;
    }

    // Independently evaluate the actual anchors/controls read by Cubism. Its
    // interactive evaluator approximates Bezier inversion, so validate stored
    // geometry tightly (1e-4), and bound native playback separately by 0.5% of
    // the local control-polygon value range. Linear/step curves stay at 1e-4.
    private static double[] exactSequenceValue(com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF attr, double frame) {
        if (!(attr.getValueData() instanceof com.live2d.cubism.doc.animation.movie.effect.attr.value.CMutableSequence)) {
            return new double[]{attr.getValueData().getDoubleValue(frame), 0};
        }
        com.live2d.cubism.doc.animation.movie.effect.attr.value.CMutableSequence seq = (com.live2d.cubism.doc.animation.movie.effect.attr.value.CMutableSequence)attr.getValueData();
        com.live2d.cubism.doc.animation.movie.effect.attr.value.CBezierPt[] points = seq.getPoints();
        if (frame <= points[0].getPos()) return new double[]{points[0].getAnchor$cubism().getDoubleValue(), 0};
        for (int n = 0; n + 1 < points.length; n++) {
            if (frame < points[n+1].getPos()) {
                double a = points[n].getAnchor$cubism().getDoubleValue();
                double d = points[n+1].getAnchor$cubism().getDoubleValue();
                String type = seq.getCurveTypes().get(n).toString();
                if (type.equals("STEP")) return new double[]{a, 0};
                if (type.equals("INVERSE_STEP")) return new double[]{frame == points[n].getPos() ? a : d, 0};
                if (type.equals("LINEAR")) return new double[]{a + (d-a)*(frame-points[n].getPos())/(points[n+1].getPos()-points[n].getPos()), 0};
                if (!type.equals("BEZIER")) throw new AssertionError("Unsupported reference curve: " + type);
                double b = points[n].getNext$cubism().getDoubleValue();
                double c = points[n+1].getPrev$cubism().getDoubleValue();
                double low = 0, high = 1;
                for (int iteration = 0; iteration < 48; iteration++) {
                    double t = (low+high)*0.5;
                    double x = cubic(points[n].getPos(), points[n].getNext$cubism().getPosF(), points[n+1].getPrev$cubism().getPosF(), points[n+1].getPos(), t);
                    if (x < frame) low = t; else high = t;
                }
                return new double[]{cubic(a,b,c,d,(low+high)*0.5), Math.max(Math.max(a,b),Math.max(c,d))-Math.min(Math.min(a,b),Math.min(c,d))};
            }
        }
        return new double[]{points[points.length-1].getAnchor$cubism().getDoubleValue(), 0};
    }

    private static String curveDetails(com.live2d.cubism.doc.animation.movie.effect.attr.CMvAttrF attr, double frame) {
        if (!(attr.getValueData() instanceof com.live2d.cubism.doc.animation.movie.effect.attr.value.CMutableSequence)) return "";
        com.live2d.cubism.doc.animation.movie.effect.attr.value.CMutableSequence seq = (com.live2d.cubism.doc.animation.movie.effect.attr.value.CMutableSequence)attr.getValueData();
        com.live2d.cubism.doc.animation.movie.effect.attr.value.CBezierPt[] points = seq.getPoints();
        for (int n=0;n+1<points.length;n++) {
            if (points[n].getPos() <= frame && frame < points[n+1].getPos()) {
                return " curve=" + seq.getCurveTypes().get(n) + " anchor=" + points[n].getPos() + "," + points[n].getAnchor$cubism().getDoubleValue()
                    + " next=" + points[n].getNext$cubism().getPosF() + "," + points[n].getNext$cubism().getDoubleValue()
                    + " prev=" + points[n+1].getPrev$cubism().getPosF() + "," + points[n+1].getPrev$cubism().getDoubleValue()
                    + " end=" + points[n+1].getPos() + "," + points[n+1].getAnchor$cubism().getDoubleValue();
            }
        }
        return "";
    }

}
