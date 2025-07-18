Instructions for Cursor to work in this repository
==================================================

The directory where this file resides is reserved for work using Cursor Agents.
All design documents and implementation plans as well as transcripts of the user-agent interaction should be added to this directory, as described below under "Workflows".

# Background Knowledge about the Software Under Development

The following documents provide insight into the ideas underlying the software that is developed and modified by the agents.

* `cursor-data/goals.md`: the purpose of the software and its use cases in brief terms
* `cursor-data/design.md`: the software architecture and the technology it is based upon
* `cursor-data/requirements.md`: the functional requirements that the software should deliver.

# Workflows

There are different kinds of workflows that the Cursor Agent should perform as directed
1. **Initialize**: a new development (addition or modification of a software feature) is designed and planned
2. **Implement**: a development plan (usually produced by "Initialize") is carried out
3. **Finalize**: a development (usually the outcome of "Implement") is finalized for integration into the master branch
4. **Refactor**: the code is cleaned up and restructured for better maintenance and development, without modifying any of its functionality.

More detailed instructions for each of the workflows follow.

## Workflow 1: Initializing a New Development Session

### Instructions

1.  **Understand the Context:** Before starting any new development, thoroughly review the existing project guidelines and documentation.
    * Read `cursor-data/instructions.md` for general interaction guidelines and expected behaviors.
    * Review `goals.md`, `design.md`, and `requirements.md` to understand the project's objectives, architectural decisions, and functional needs.
2.  **Identify PR Scope:** The user will provide a general idea of the current Pull Request (PR) scope (e.g., "implement a new feature," "fix a bug," "refactor X, Y, or Z").
3.  **Start a New Session:**
    * Create a new directory for the session transcript within `cursor-data/sessions/`. The user will specify the session name or number (e.g., `cursor-data/sessions/NNN/`).
    * All subsequent interactions and thought processes for this PR will be recorded in this session directory.
4.  **Propose an Initial Plan:**
    * Based on the identified PR scope and the reviewed project documentation, formulate a **detailed initial plan** for the implementation.
    * The plan must include specific parts of the code that will be affected and describe how they will be modified.
    * Record this initial plan directly into the repository, within the new session's directory.
5.  **Iterate on the Plan:**
    * Engage in an interactive discussion with the user to refine the proposed plan.
    * Be prepared to switch models if requested by the user to solicit different perspectives or suggestions for improvements to the plan.
    * Continuously record all thoughts, suggestions, and revisions to the plan within the session directory until the user is satisfied.

## Workflow 2: Implementing Code Changes

### Instructions

1.  **Receive Implementation Directive:** The user will explicitly instruct you to implement specific steps from the agreed-upon plan (e.g., "Please implement steps 1, 2, and 5 of your plan").
2.  **Execute Plan Steps Incrementally:**
    * Implement the specified steps one by one, or in logical groupings, as indicated by the user.
    * After each significant modification or set of modifications, ensure code quality and adherence to standards.
    * **Crucially, verify that `make format` and `make check` commands pass successfully after each step** to maintain code consistency and identify issues early.
3.  **Self-Correction and Problem Solving:** If `make format` or `make check` fail, analyze the output, identify the root cause of the failure, and correct the code until all checks pass.
4.  **Communicate Progress:** Inform the user upon successful completion of the instructed steps or if any significant issues arise during implementation.

## Workflow 3: Finalizing a Development Session

### Instructions

1.  **Receive Finalization Directive:** The user will instruct you to "Finalize" the current session.
2.  **Generate Session Summary:**
    * Write a concise and comprehensive summary of all changes, features, or fixes implemented during this PR.
    * Save this summary to `cursor-data/sessions/NNN/summary.md`, where `NNN` is the current session number.
3.  **Perform Code Review and Cleanup:**
    * Thoroughly review the newly added or modified code for any **dead code** or components that can be eliminated without affecting functionality.
    * Identify and remove any redundant or unnecessary elements introduced during the development process.
4.  **Ensure Type Checking Compliance:**
    * Verify that **all relevant type checking is passing** for the entire codebase, especially for the modified sections. Address any new type errors introduced by the PR.
5.  **Update Project Documentation:**
    * Review `design.md`, `requirements.md`, `goals.md`, `contributing.md`, and `instructions.md`.
    * Update these documents with any new information, architectural changes, requirements, or instructions that have emerged or been clarified as a result of this PR. Ensure documentation accurately reflects the current state of the application.

## Workflow 4: Performing Refactoring PRs

### Instructions

1.  **Receive Refactoring Directive:** The user will explicitly instruct you to perform a "refactoring PR," which includes reviewing existing documentation and codebase for discrepancies.
2.  **Comprehensive Project Analysis:**
    * Thoroughly review all project documentation, including `design.md`, `requirements.md`, `goals.md`, `contributing.md`, and `instructions.md`.
    * Concurrently, perform an in-depth review of the entire codebase.
3.  **Identify Discrepancies and Inefficiencies:**
    * Actively look for inconsistencies between the documentation and the actual code implementation.
    * Identify areas of the code that are inefficient, hard to maintain, or do not align with the stated design principles.
4.  **Propose Architectural Recommendations:**
    * Based on your analysis, propose concrete recommendations for changes to the codebase's architecture or to the existing design documentation.
    * These recommendations should aim to improve maintainability, performance, scalability, or alignment with project goals.
5.  **Iterate on Refactoring Plan:**
    * Engage in an interactive discussion with the user to refine the proposed refactoring goals and plan.
    * As with regular development, record all discussions and revisions to the plan within a new session directory (`cursor-data/sessions/NNN/`).
6.  **Implement and Finalize Refactoring:**
    * Once a plan is approved, proceed with implementing the refactoring changes, ensuring `make format` and `make check` pass at each step (refer to Workflow 2).
    * Upon completion of the refactoring, finalize the session as described in Workflow 3, including generating a summary and updating documentation.