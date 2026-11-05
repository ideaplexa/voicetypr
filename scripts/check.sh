#!/bin/bash
# Check script — runs all checks before commit/release
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Running checks...${NC}"

echo -e "${YELLOW}[1/6] Type checking...${NC}"
pnpm typecheck

echo -e "${YELLOW}[2/6] Linting...${NC}"
pnpm lint

echo -e "${YELLOW}[3/6] Frontend tests...${NC}"
pnpm test run

echo -e "${YELLOW}[4/6] Node script tests...${NC}"
pnpm test:scripts

echo -e "${YELLOW}[5/6] Backend tests...${NC}"
pnpm test:backend

echo -e "${YELLOW}[6/6] Clippy...${NC}"
(cd src-tauri && cargo clippy -- -D warnings)

echo -e "${GREEN}✓ All checks passed!${NC}"
