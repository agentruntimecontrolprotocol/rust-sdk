.PHONY: docs-api docs-api-clean

# Regenerate per-crate Markdown API summaries under docs/api/.
# Output is consumed by the www site at build time.
docs-api:
	@python3 scripts/gen-api-docs.py

docs-api-clean:
	@rm -rf docs/api
