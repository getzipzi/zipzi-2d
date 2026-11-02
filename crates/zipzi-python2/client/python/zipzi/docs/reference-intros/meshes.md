Create meshes with edit.meshes.create(MeshSpec(...)); read them with session.meshes.get(id). Mesh IDs and vertex IDs are separate identities, and triangle entries contain vertex IDs. Positions use model space without a deformation parent, otherwise the parent local/parameter space. UVs describe texture coordinates.

Values own their collections; constructing or reading them performs no host IO.
