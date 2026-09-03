# Publishing `@cubiczan/autonomous-business-os-mcp`

Packaging is prepared. **Do not publish to npm or PyPI from an agent run** unless a human explicitly asks.

When a maintainer is ready:

1. Confirm the stdio tests pass (`pytest tests/test_mcp.py`).
2. From `mcp/`, review `package.json` version and `server.json`.
3. `npm publish --access public` (human-owned npm token).
4. Optional PyPI extra remains `pip install -e ".[mcp]"` until a dedicated distribution is cut.

The npm bin is a launcher: it execs `python -m app.mcp` against `ABOS_REPO_ROOT` or a local checkout.
