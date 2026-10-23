"""Run against an installed wheel from outside the source tree."""

import unittest

if __package__:
    from .cpu.test_motion_animation import CpuMotionAnimationTests
    from .cpu.test_session_lifecycle import CpuSessionLifecycleTests
    from .cpu.test_hierarchy_features import CpuHierarchyFeaturesTests
    from .cpu.test_mesh_geometry import CpuMeshGeometryTests
else:
    from cpu.test_motion_animation import CpuMotionAnimationTests
    from cpu.test_session_lifecycle import CpuSessionLifecycleTests
    from cpu.test_hierarchy_features import CpuHierarchyFeaturesTests
    from cpu.test_mesh_geometry import CpuMeshGeometryTests


if __name__ == "__main__":
    unittest.main()
