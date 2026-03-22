# Contributing to AG-Claw

Thank you for your interest in contributing to AG-Claw! This document provides guidelines and instructions for contributing.

## 🎯 Ways to Contribute

- 🐛 Report bugs
- ✨ Suggest new features
- 📚 Improve documentation
- 🔧 Submit pull requests
- 🧪 Write tests
- 🌐 Translate into other languages

## 🚀 Getting Started

### 1. Fork the Repository

Fork the repository on GitHub and clone your fork locally:

```bash
git clone https://github.com/YOUR_USERNAME/ag-claw.git
cd ag-claw
```

### 2. Install Dependencies

```bash
npm install
```

### 3. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

## 📝 Coding Standards

### Code Style

We use ESLint and Prettier for code formatting:

```bash
# Check formatting
npm run format:check

# Fix formatting issues
npm run format
```

### TypeScript

- Use strict TypeScript mode
- Avoid `any` type
- Provide proper types for all functions and variables
- Document complex logic with comments

### Commit Messages

We follow Conventional Commits:

```
feat: add new feature
fix: resolve bug
docs: update documentation
style: formatting changes
refactor: code refactoring
test: add tests
chore: maintenance tasks
```

Example:
```
feat: add WebSocket support for real-time agent communication

Adds WebSocket endpoint for monitoring agent activity in real-time.
Includes connection pooling and automatic reconnection logic.
```

## 🧪 Testing

```bash
# Run all tests
npm test

# Run with coverage
npm run test:coverage

# Run specific test suite
npm run test:unit
npm run test:integration
```

### Writing Tests

- Write tests for all new features
- Ensure all tests pass before submitting PR
- Aim for meaningful test coverage

## 🔍 Making Changes

1. Make your changes in your feature branch
2. Add or update tests as needed
3. Ensure code follows our style guidelines
4. Write clear commit messages
5. Push to your fork and submit a pull request

## 📬 Pull Request Process

1. Fill out the PR template completely
2. Request review from maintainers
3. Address any feedback
4. Once approved, your PR will be merged

## 🔒 Security

If you discover a security vulnerability, please DO NOT open a public issue. Instead, email us privately or see [SECURITY.md](./SECURITY.md).

## 📜 License

By contributing, you agree that your contributions will be licensed under the MIT License.

## 🙏 Thank You

Every contribution is appreciated. Thank you for making AG-Claw better!

## 📞 Getting Help

- 📖 Check [docs/](docs/) for documentation
- 💬 Join our [Telegram channel](https://t.me/agclaw)
- 🐛 Open an issue for bugs
- ✨ Submit feature requests
