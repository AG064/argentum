#!/usr/bin/env node
/**
 * Generates Security Fixes section for release notes
 * Searches git log for commits with security-related keywords
 */

import { execSync } from 'child_process';
import { readFileSync, writeFileSync } from 'fs';

const VERSION = process.argv[2] || process.env.GITHUB_REF_NAME || 'v0.0.0';
const TAG_PATTERN = `v${VERSION.replace('v', '')}`;

function getSecurityCommits() {
  try {
    // Get commits since last tag or last 50 commits
    const log = execSync(
      `git log --oneline --grep="security" --grep="CVE" --grep="fix" --grep="vuln" --grep="exploit" -i --since="2024-01-01" -100`,
      { encoding: 'utf8' },
    );
    return log.trim().split('\n').filter(Boolean);
  } catch {
    return [];
  }
}

function getCommitDetails(sha) {
  try {
    const msg = execSync(`git log -1 --format="%s%n%b" ${sha}`, { encoding: 'utf8' });
    const files = execSync(`git diff-tree --no-commit-name --name-only -r ${sha}`, {
      encoding: 'utf8',
    });
    return { message: msg.trim(), files: files.trim().split('\n').filter(Boolean) };
  } catch {
    return { message: '', files: [] };
  }
}

function generateSecuritySection() {
  const commits = getSecurityCommits();

  if (commits.length === 0) {
    return '';
  }

  let section = `\n## Security Fixes\n\n`;
  section += `The following security-related changes were made in ${VERSION}:\n\n`;

  const fixes = [];
  for (const line of commits) {
    const [sha, ...descParts] = line.split(' ');
    const desc = descParts.join(' ');

    // Only include if it looks like a security fix
    const lowerDesc = desc.toLowerCase();
    if (lowerDesc.includes('security') || lowerDesc.includes('cve') || lowerDesc.includes('vuln')) {
      fixes.push({ sha, desc });
    }
  }

  if (fixes.length === 0) {
    return '';
  }

  for (const { sha, desc } of fixes) {
    section += `- ${desc} ([${sha.slice(0, 7)}](https://github.com/AG064/argentum/commit/${sha}))\n`;
  }

  return section;
}

// Output to stdout for use in CI
const section = generateSecuritySection();
if (section) {
  console.log(section);
}
