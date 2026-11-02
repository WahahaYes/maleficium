# The full local gate: every prek hook on every file, then the full npm
# check (lint, format, types, vitest, cargo fmt/clippy/test). CI runs the
# same commands directly — the precommit job's `prek run --all-files` plus
# `npm run check` in the fast_check and package legs — so this target
# documents that equivalence without rewiring any job (CI cost unchanged).
SHELL := /bin/bash
.DEFAULT_GOAL := help

##@ Checks

.PHONY: check
check: ## Run prek on all files, then the full npm check gate
	prek run --all-files --show-diff-on-failure
	npm run check

##@ Help

.PHONY: help
help: ## Show this help message
	@awk 'BEGIN {FS = ":.*?## "}; /^##@ / {printf "\n\033[1m%s\033[0m\n", substr($$0, 5)}; /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
