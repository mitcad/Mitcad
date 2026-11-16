# SPDX-License-Identifier: MIT
# The live MCP endpoint's limits against a hostile local client
# (mitcad#935): refusal echoes, the log ring's bytes, the connection
# slots and the token's comparison; plain Qt Core and the log ring.
target_sources(test_app PRIVATE tests/mcp_endpoint_limits.cpp mcp/EndpointLimits.cpp mcp/EndpointLimits.hpp)
