"""Run against an installed wheel from outside the source tree."""

import unittest

if __package__:
    from .cpu.common import *
    from .cpu.test_motion_animation import CpuMotionAnimationTests
    from .cpu.test_session_lifecycle import CpuSessionLifecycleTests
    from .cpu.test_hierarchy_features import CpuHierarchyFeaturesTests
    from .cpu.test_mesh_geometry import CpuMeshGeometryTests
else:
    from cpu.common import *
    from cpu.test_motion_animation import CpuMotionAnimationTests
    from cpu.test_session_lifecycle import CpuSessionLifecycleTests
    from cpu.test_hierarchy_features import CpuHierarchyFeaturesTests
    from cpu.test_mesh_geometry import CpuMeshGeometryTests


class CpuWheelTests(
    CpuMotionAnimationTests,
    CpuSessionLifecycleTests,
    CpuHierarchyFeaturesTests,
    CpuMeshGeometryTests,
):
    pass


def load_tests(loader, tests, pattern):
    suite = unittest.TestSuite()
    for test_class in [
        CpuMotionAnimationTests,
        CpuSessionLifecycleTests,
        CpuHierarchyFeaturesTests,
        CpuMeshGeometryTests,
    ]:
        suite.addTests(loader.loadTestsFromTestCase(test_class))
    return suite


if __name__ == "__main__":
    unittest.main()
