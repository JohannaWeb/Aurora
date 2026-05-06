IMAGE ?= aurora
PROJECTS_DIR := $(abspath ..)
DOCKERFILE := $(abspath Dockerfile)
SCREENSHOT ?= /tmp/google-homepage.png
FIXTURE ?= google-homepage
VIEWPORT_WIDTH ?= 1338
VIEWPORT_HEIGHT ?= 786
SCREENSHOT_DIR := $(dir $(abspath $(SCREENSHOT)))
SCREENSHOT_FILE := $(notdir $(SCREENSHOT))

.PHONY: docker-build docker-run docker-fixture docker-x11 docker-screenshot screenshot mockup-screenshot check-line-cap

docker-build:
	docker build -f $(DOCKERFILE) -t $(IMAGE) $(PROJECTS_DIR)

docker-run:
	docker run --rm $(IMAGE) $(ARGS)

docker-fixture:
	docker run --rm $(IMAGE) --fixture google-homepage

docker-x11:
	docker run --rm \
		-e DISPLAY \
		-v /tmp/.X11-unix:/tmp/.X11-unix \
		$(IMAGE) $(ARGS)

docker-screenshot:
	docker run --rm \
		-e AURORA_SCREENSHOT=/out/$(SCREENSHOT_FILE) \
		-v $(SCREENSHOT_DIR):/out \
		$(IMAGE) --fixture google-homepage

screenshot:
	AURORA_VIEWPORT_WIDTH=$(VIEWPORT_WIDTH) \
	AURORA_VIEWPORT_HEIGHT=$(VIEWPORT_HEIGHT) \
	AURORA_SCREENSHOT_WIDTH=$(VIEWPORT_WIDTH) \
	AURORA_SCREENSHOT_HEIGHT=$(VIEWPORT_HEIGHT) \
	AURORA_SCREENSHOT=$(SCREENSHOT) \
	cargo run -- --fixture $(FIXTURE)

mockup-screenshot:
	$(MAKE) screenshot FIXTURE=aurora-search SCREENSHOT=/tmp/aurora-search.png VIEWPORT_WIDTH=1338 VIEWPORT_HEIGHT=786

check-line-cap:
	@find src -name '*.rs' -exec wc -l {} + | awk '$$1 > 200 && $$2 != "total" { print $$1, $$2; bad=1 } END { if (bad) { print "FAIL: files above 200-line cap"; exit 1 } else { print "OK: all src files within 200-line cap" } }'
