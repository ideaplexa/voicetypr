#!/bin/bash
# Check script — runs all checks before commit/release
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Running checks...${NC}"

echo -e "${YELLOW}[1/5] Type checking...${NC}"
pnpm typecheck

echo -e "${YELLOW}[2/5] Linting...${NC}"
pnpm lint

echo -e "${YELLOW}[3/5] Frontend tests...${NC}"
pnpm test run

echo -e "${YELLOW}[4/5] Backend tests...${NC}"
pnpm test:backend

echo -e "${YELLOW}[5/5] Clippy...${NC}"
(cd src-tauri && cargo clippy -- -D warnings)

echo -e "${GREEN}✓ All checks passed!${NC}"
