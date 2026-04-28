# Docker

Aurora depends on the sibling `Opus` crate through `../Opus`, so build the image
from the parent `projects` directory:

```bash
cd ..
docker build -f Aurora/Dockerfile -t aurora .
```

Or from the Aurora directory:

```bash
make docker-build
```

Run the bundled fixture:

```bash
docker run --rm aurora --fixture google-homepage
```

Or:

```bash
make docker-fixture
```

Run with an X11 display from Linux:

```bash
docker run --rm \
  -e DISPLAY \
  -v /tmp/.X11-unix:/tmp/.X11-unix \
  aurora --fixture google-homepage
```

Save a screenshot to the host:

```bash
docker run --rm \
  -e AURORA_SCREENSHOT=/out/google-homepage.png \
  -v /tmp:/out \
  aurora --fixture google-homepage
```

Or:

```bash
make docker-screenshot SCREENSHOT=/tmp/google-homepage.png
```
