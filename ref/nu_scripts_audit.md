# nu_scripts Formatting Audit

Ran nufmt v0.6.0 against [nushell/nu_scripts](https://github.com/nushell/nu_scripts) repository.

## Summary

- **Total files:** 1529
- **Formatted:** 1462
- **Unchanged:** 33
- **Failed (parse errors):** 34

The 34 failures are all Nushell parse errors - files using deprecated or incompatible syntax (old Nushell versions, missing commands, etc.). These are expected and not nufmt bugs.

## Bugs Found

### BUG-001: External command caret (^) separated from command name

**Severity:** Critical (breaks code)

When an external command starts a pipeline with `^`, the caret gets separated from the command name:

```diff
 export def git_current_branch [] {
-    ^git rev-parse --abbrev-ref HEAD
+  ^
+
+  git rev-parse --abbrev-ref HEAD
 }
```

```diff
 def "nu-complete aerospace-list-all-workspaces" [] {
-    ^aerospace list-workspaces --all
-    | lines
+  ^
+  aerospace list-workspaces --all
+  | lines
 }
```

This produces invalid Nushell code. The `^` operator must be attached to the command name.

**Files affected:** Many, including:
- `aliases/git/git-aliases.nu`
- `custom-completions/aerospace/aerospace-completions.nu`

---

### BUG-002: Match expressions collapsed to single line incorrectly

**Severity:** High (code style, potential readability issues)

Multiline match expressions are collapsed to a single line when they fit, but the closing brace ends up on its own line:

```diff
 def fib [n: int] {
-  match $n {
-    0 => 0,
-    1 => 1,
-    $n => { (fib ($n - 1)) + (fib ($n - 2)) },
+  match $n { 0 => 0, 1 => 1, $n => { (fib ($n - 1)) + (fib ($n - 2)) },
   }
 }
```

The closing `}` is orphaned on its own line, which is inconsistent formatting.

**Files affected:**
- `benchmarks/fibonacci-recursive.nu`
- Many files with match expressions

---

### BUG-003: String interpolations with parentheses break across lines

**Severity:** Critical (breaks code)

String interpolations containing `(char lp)` or similar subcommands are being broken across lines inside the string:

```diff
-export alias glod = git log --graph $'--pretty=%Cred%h%Creset -%C(char lp)auto(char rp)%d%Creset %s %Cgreen(char lp)%ad(char rp) %C(char lp)bold blue(char rp)<%an>%Creset'
+export alias glod = git log --graph $'--pretty=%Cred%h%Creset -%C(char lp)auto(char rp)
+
+  %d%Creset %s %Cgreen(char lp)%ad(char rp) %C(char lp)bold blue(char rp)<%an>%Creset'
```

This breaks the string literal by inserting newlines inside it.

**Files affected:**
- `aliases/git/git-aliases.nu`

---

### BUG-004: Closure with comment after `||` breaks incorrectly

**Severity:** Medium (formatting issue)

When a closure has a comment immediately after the parameter list, the comment gets moved:

```diff
-seq 0 $height | par-each {|| # create these in parallel
-    let row_data = (seq 0 $width | each { |col|
+seq 0 $height | par-each {||
+  # create these in parallel
+  let row_data = (seq 0 $width | each {|col|
```

The comment "create these in parallel" was on the same line as the closure opening, but gets moved to a new line. While this may be intentional, it changes the semantic meaning (inline comment vs line comment).

**Files affected:**
- `benchmarks/gradient-autoview.nu`

---

## Non-Bug Observations

### Intentional formatting changes (working as designed):

1. **Trailing commas added** - Lists and records now get trailing commas:
   ```diff
   -  [ "toggle", "on", "off" ]
   +  [ "toggle", "on", "off" ]
   ```
   (Note: Actually these look the same in single-line mode, only multiline gets commas)

2. **Quote normalization** - Single quotes converted to double quotes where safe:
   ```diff
   -    ['toggle', 'on', 'off']
   +  [ "toggle", "on", "off" ]
   ```

3. **Indentation normalization** - 4-space indent converted to 2-space

4. **Trailing whitespace removed** - Lines with trailing spaces cleaned up

5. **Trailing newlines added** - Files missing final newline get one

6. **Pipeline formatting** - Pipelines reformatted with `|` at start of line:
   ```diff
   -    git remote show origin
   -        | lines
   -        | str trim
   +  git remote show origin
   +  | lines
   +  | str trim
   ```

---

## Recommendations

### Priority 1 (Critical - breaks code):
1. **BUG-001:** Keep `^` attached to command name - never insert newlines after caret
2. **BUG-003:** Never break inside string literals, even interpolated ones

### Priority 2 (High):
3. **BUG-002:** Either keep match expressions multiline, or format closing brace correctly

### Priority 3 (Medium):
4. **BUG-004:** Decide on comment handling policy for inline comments in closures
