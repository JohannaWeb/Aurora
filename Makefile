IMAGE ?= aurora
PROJECT_DIR := $(abspath .)
DOCKERFILE := $(abspath Dockerfile)
SCREENSHOT_ROOT ?= tests/screenshots
SCREENSHOT ?= $(SCREENSHOT_ROOT)/google-homepage.png
FIXTURE ?= google-homepage
VIEWPORT_WIDTH ?= 1338
VIEWPORT_HEIGHT ?= 786
SCREENSHOT_DIR := $(dir $(abspath $(SCREENSHOT)))
SCREENSHOT_FILE := $(notdir $(SCREENSHOT))

.PHONY: docker-build docker-run docker-fixture docker-x11 docker-screenshot screenshot mockup-screenshot dynamic-screenshot raf-screenshot all-renders check-line-cap

docker-build:
	docker build -f $(DOCKERFILE) -t $(IMAGE) $(PROJECT_DIR)

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
	$(MAKE) screenshot FIXTURE=aurora-search SCREENSHOT=$(SCREENSHOT_ROOT)/aurora-search.png VIEWPORT_WIDTH=1338 VIEWPORT_HEIGHT=786

dynamic-screenshot:
	$(MAKE) screenshot FIXTURE=dynamic-reflow SCREENSHOT=$(SCREENSHOT_ROOT)/dynamic-reflow.png VIEWPORT_WIDTH=900 VIEWPORT_HEIGHT=620

raf-screenshot:
	$(MAKE) screenshot FIXTURE=raf-reflow SCREENSHOT=$(SCREENSHOT_ROOT)/raf-reflow.png VIEWPORT_WIDTH=900 VIEWPORT_HEIGHT=620

all-renders:
	$(MAKE) screenshot FIXTURE=google-homepage SCREENSHOT=$(SCREENSHOT_ROOT)/google-homepage.png VIEWPORT_WIDTH=1338 VIEWPORT_HEIGHT=786
	$(MAKE) screenshot FIXTURE=aurora-search SCREENSHOT=$(SCREENSHOT_ROOT)/aurora-search.png VIEWPORT_WIDTH=1338 VIEWPORT_HEIGHT=786
	$(MAKE) screenshot FIXTURE=demo SCREENSHOT=$(SCREENSHOT_ROOT)/demo.png VIEWPORT_WIDTH=1200 VIEWPORT_HEIGHT=900
	$(MAKE) screenshot FIXTURE=dynamic-reflow SCREENSHOT=$(SCREENSHOT_ROOT)/dynamic-reflow.png VIEWPORT_WIDTH=900 VIEWPORT_HEIGHT=620
	$(MAKE) screenshot FIXTURE=raf-reflow SCREENSHOT=$(SCREENSHOT_ROOT)/raf-reflow.png VIEWPORT_WIDTH=900 VIEWPORT_HEIGHT=620

update-snapshots:
	UPDATE_SNAPSHOTS=1 cargo test --test visual_regression

check-snapshots:
	cargo test --test visual_regression

check-line-cap:
	@find src -name '*.rs' -exec wc -l {} + | awk '$$1 > 200 && $$2 != "total" { print $$1, $$2; bad=1 } END { if (bad) { print "FAIL: files above 200-line cap"; exit 1 } else { print "OK: all src files within 200-line cap" } }'
