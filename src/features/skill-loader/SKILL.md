# Skill Loader Feature

Loads SKILL.md files from AG-Claw feature directories and injects them into agent context.

This is the **OpenFang "Hand" pattern** - domain expertise bundled with features.

## How it works

Each feature can have a `SKILL.md` file in its directory. This file contains domain-specific knowledge that gets injected into the agent's context at runtime.

## SKILL.md Format

```markdown
# Feature Name

## Overview
Brief description of what this skill provides.

## Tools
- tool1: description
- tool2: description

## Patterns
How to use this skill effectively.
```

## Adding a skill to your feature

1. Create `SKILL.md` in your feature directory
2. Add domain-specific knowledge, patterns, and examples
3. The skill is automatically loaded when the feature is enabled

## Usage

```typescript
import { skillLoader } from './features/skill-loader';

// Get all loaded skills
const skills = skillLoader.getLoadedSkills();

// Inject skills for specific features into context
const context = skillLoader.injectSkillsIntoContext(
  ['./features/telegram', './features/email-integration'],
  { existing: 'context' }
);
```

## API

- `getLoadedSkills()`: Returns all loaded SkillContext objects
- `getSkillsAsText()`: Returns all skills as formatted markdown string
- `injectSkillsIntoContext(dirs, context)`: Inject skills for specific directories
- `loadSkillFromFeature(dir)`: Load a single skill from a feature directory
