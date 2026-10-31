# GH-1341 isolated merge-queue preflight canary

This disposable documentation-only commit tests whether GitHub's native merge queue admits a pull request whose required GitHub Actions check exists on the **synthetic PR test-merge SHA**, and is absent from the source PR head SHA.

The temporary target branch has independent `pull_request` witness and synthetic-only `push` / `merge_group` workflows. The base and source branches are disposable, and protected production main is not a target.

The expected evidence is the exact App ID and SHA of the required check, followed by a native enqueue result and, if admitted, a staging SHA receipt.
