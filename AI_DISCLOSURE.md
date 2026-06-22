## 🤖 AI Disclosure and Project Policy

This project is an experiment in AI-driven software engineering. Almost all aspects of this programming language, including its design, interpreter, and compiler, were generated using artificial intelligence.

## 🛠️ AI Tools Used

- ChatGPT & Claude: Used for high-level language design, syntax choices, architecture planning, and core logic.
- GitHub Copilot: Used for inline code generation, rapid prototyping, and writing tests.
- Gemini: Used for debugging, code optimization, and alternative architectural ideas.

## ⚖️ Licensing and AI Disclaimer (MIT License)

This project is licensed under the MIT License. Because this software is almost entirely AI-generated, please note the following terms:

- No Warranty (As-Is): As per the MIT License, this code is provided "as is". AI models can make mistakes, introduce security bugs, or hallucinate logic. Use this compiler/interpreter at your own risk.
- Data Origin: All code was generated using public, commercial AI tools (ChatGPT, Claude, Copilot, Gemini). While we believe the output is safe to use under MIT terms, we cannot track every training data source used by these third-party companies. [5, 6]
- Copyright Limits: Current laws vary on whether AI-generated code can hold a copyright. By using this project, you agree that you are free to use it under the MIT License, but the creators make no claims of unique copyright ownership over the raw AI outputs. [7, 8]

## 📊 Code Composition

- ~90%+ AI-Generated: The foundational architecture, compiler phases (lexing, parsing, AST generation), and runtime environment were produced by AI prompts.
- ~10% Human-Refined: Human input was limited to prompting, chaining modules together, setting project goals, and high-level debugging.

---

## 👥 Policy for External Contributors

We welcome human and AI-assisted contributions! However, to keep our repository history clean and transparent, please follow these rules:

## 1. Disclose AI Assistance

If you use AI to write a pull request (PR) or a fix, you must declare it. Please append a Git Trailer to your commit message or note it clearly in your PR description.
Example Commit Message:

Fix parser bug with nested loops

Generated-by: Claude <noreply@anthropic.com>
Co-authored-by: Copilot <copilot@github.com>

## 2. Verify Before Submitting

AI can make mistakes or invent fake features (hallucinations).

- Run tests: Ensure your code passes all existing test suites.
- Review line-by-line: Do not submit blind AI outputs. You must understand the code you submit.

## 3. No Autonomous Agent PRs

We do not accept automated PRs sent directly by AI bots or autonomous coding agents without human review and oversight.
