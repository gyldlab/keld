/**
 * The pre-entry installer a Host-selected preload would run (GH-570 spec T2).
 * It imports only the facts owner: no kipc, no facade, no resolution of
 * application code. The facade import that follows finds identical
 * descriptors and installs nothing.
 */
import { installMainProcessFacts } from "../src/process-facts.ts";

installMainProcessFacts();
