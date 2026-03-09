/**
 RalphModels

 Responsibilities:
 - Act as the thin facade for RalphCore model surfaces.
 - Keep the historic entrypoint lightweight while concrete model types live in dedicated files.

 Does not handle:
 - Defining the CLI spec, JSON, or task models directly.
 - Any CLI execution or workspace behavior.

 Invariants/assumptions callers must respect:
 - Concrete model declarations live in adjacent `Ralph*` model files within RalphCore.
 */

import Foundation
