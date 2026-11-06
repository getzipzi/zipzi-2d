"""Create a parameter and save a fresh native project using the Local backend."""
from pathlib import Path
from uuid import uuid4

import zipzi
from zipzi import values as v


def create_project(destination: Path) -> None:
    with zipzi.local() as runtime:
        session = runtime.sessions.create()
        parameter = str(uuid4())
        with session.draft("Create Angle") as draft:
            draft.apply(v.CommandCreateParameter(spec=v.ParameterCreateSpec(
                id=parameter, name="Angle", minimum=-30, maximum=30, default_value=0,
            )))
        snapshot = session.query(v.QueryObjects(addresses=(
            v.ObjectAddress(kind=v.ObjectKind.PARAMETER, id=parameter),
        )))
        assert snapshot.value["records"][0]["value"]["name"] == "Angle"
        with session.save(destination) as job:
            assert job.wait()["published"]


if __name__ == "__main__":
    import sys

    create_project(Path(sys.argv[1]).absolute())
