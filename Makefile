COMPOSE := docker compose

.PHONY: all
all: static test down

.PHONY: static
static: fmt-check check clippy

.PHONY: fmt
fmt:
	cargo fmt --all

.PHONY: fmt-check
fmt-check:
	cargo fmt --all -- --check

.PHONY: check
check:
	cargo check --all-targets

.PHONY: clippy
clippy:
	cargo clippy --all-targets -- -D warnings
	
.PHONY: test
test: up
	cargo test

.PHONY: up
up:
	$(COMPOSE) up -d --wait
	@echo
	@echo "A 2-node cluster is running in the background. Use 'make down' to stop it and remove all the volumes."
	@echo

.PHONY: down
down:
	$(COMPOSE) down --remove-orphans -v

.PHONY: logs
logs:
	$(COMPOSE) logs -f

.PHONY: cqlsh
cqlsh:
	$(COMPOSE) exec scylla1 cqlsh -u cassandra -p cassandra

.PHONY: shell
shell:
	$(COMPOSE) exec scylla1 bash

.PHONY: volumes
volumes:
	docker volume ls

.PHONY: prune
prune:
	docker system prune -a --volumes

.PHONY: clean
clean: down
	cargo clean