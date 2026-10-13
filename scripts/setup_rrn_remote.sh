#!/bin/bash
# setup_rrn_remote.sh - Set up the RRN public remote and push experimental branch
#
# Usage: ./scripts/setup_rrn_remote.sh
# Example: ./scripts/setup_rrn_remote.sh

set -e

GITHUB_USERNAME="anusornc"
RRN_REPO_NAME="SPACL-RRN"
RRN_REMOTE_URL="git@github.com:${GITHUB_USERNAME}/${RRN_REPO_NAME}.git"

echo "========================================"
echo "RRN Remote Setup Script"
echo "========================================"
echo ""

# Check current branch
CURRENT_BRANCH=$(git branch --show-current)
echo "Current branch: $CURRENT_BRANCH"

if [ "$CURRENT_BRANCH" != "exp/hybrid-rrn-paper" ]; then
    echo ""
    echo "WARNING: You are not on exp/hybrid-rrn-paper branch!"
    echo "Switching to exp/hybrid-rrn-paper..."
    git checkout exp/hybrid-rrn-paper
fi

# Check if rrn remote already exists
if git remote | grep -q "^rrn$"; then
    echo ""
    echo "RRN remote already exists:"
    git remote get-url rrn
    echo ""
    read -p "Do you want to update it? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 0
    fi
    git remote set-url rrn "$RRN_REMOTE_URL"
else
    echo ""
    echo "Adding rrn remote: $RRN_REMOTE_URL"
    git remote add rrn "$RRN_REMOTE_URL"
fi

# Verify remote
echo ""
echo "Verifying remote..."
git remote -v | grep rrn

# Push exp branch to rrn repo as main
echo ""
echo "========================================"
echo "Pushing exp/hybrid-rrn-paper to rrn/main..."
echo "========================================"
git push -u rrn exp/hybrid-rrn-paper:main

echo ""
echo "========================================"
echo "Setup Complete!"
echo "========================================"
echo ""
echo "Next steps:"
echo "1. Verify GitHub repo exists: ${GITHUB_USERNAME}/${RRN_REPO_NAME}"
echo "2. Run this script again to push"
echo "3. Verify at: https://github.com/${GITHUB_USERNAME}/${RRN_REPO_NAME}"
echo ""
echo "Usage after setup:"
echo "  git push rrn exp/hybrid-rrn-paper:main"
echo ""
