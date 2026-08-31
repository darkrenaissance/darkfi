## Purpose

Defines layered value resolution for the app's property system: defaults
installable on live properties (including expression defaults), the read
resolution order across set values, expressions, and defaults, and
per-index float expression evaluation with dependency-triggered
re-evaluation. This is the substrate that lets any styled or geometric
property be overridden and later restored to its baseline without
recreating nodes.

## ADDED Requirements

### Requirement: Post-creation default installation

A default value SHALL be installable on any property of a live, already
linked node, for every property type (bool, uint32, float32, string,
enum, node id, shape, and expression). Installing a default on a property
whose value is unset SHALL change the property's effective value. A value
set explicitly in the value slot SHALL take precedence over the default,
and clearing the value SHALL fall back to the installed default.

#### Scenario: Default installed on a live node is effective

- **WHEN** a default is installed on a property of a linked node whose value is unset
- **THEN** reading the property yields the installed default

#### Scenario: Explicit value overrides an installed default

- **WHEN** a property has both an installed default and an explicitly set value
- **THEN** reading the property yields the explicitly set value

#### Scenario: Clearing the value falls back to the default

- **WHEN** an explicitly set value is cleared (unset) and a default is installed
- **THEN** reading the property yields the installed default again

### Requirement: Expression defaults and effective expression source

Defaults SHALL be allowed to hold expressions. When both the value slot
and the default can supply an expression, the value slot's expression
SHALL be the active source; when the value slot holds no expression and
no plain value, the default's expression SHALL be the active source.
Evaluators SHALL evaluate whichever expression is active.

#### Scenario: Unsetting an override returns control to the default expression

- **WHEN** a property's geometry is governed by a default expression, a theme
  overrides it with a plain value or its own expression, and the override
  is then unset
- **THEN** the property is once again computed from the default expression

#### Scenario: Both slots holding expressions prefers the value slot

- **WHEN** the value slot holds an expression and the default also holds an
  expression
- **THEN** evaluation uses the value slot's expression

### Requirement: Reads never expose unresolved expressions

Reading a property's effective value SHALL never yield an expression
object. Resolution SHALL proceed: set value, then the set expression's
last computed result, then the default, then the default expression's
last computed result, then the type's neutral default. Before an
expression has been evaluated for the first time, concrete-type reads
SHALL succeed with the next available layer rather than fail.

#### Scenario: Read before first evaluation

- **WHEN** a float property holds an expression that has never been evaluated
  and no default is installed
- **THEN** reading it as a float yields the type default instead of an error

#### Scenario: Read after the expression is evaluated

- **WHEN** an expression property has been evaluated and cached
- **THEN** reading it as a float yields the cached computed result

### Requirement: Role-based property permissions

Roles SHALL be a bitflag set, and every property SHALL carry a
permission pair (readable roles, writable roles) supplied at creation.
A write attempt (set, unset, clear, expr, push, insert, remove) by a
role not in the write mask SHALL fail with a permission-denied error
and leave the property unmodified. A read by a role not in the read
mask SHALL fail the same way wherever the acting role is attributable:
wrapped property handles, expression evaluation of dependencies, and
external (RPC) property access. Default installation and evaluation
cache writes SHALL be exempt from write checks. Until factories assign
masks, a default permission allowing all roles SHALL preserve current
behavior.

#### Scenario: Denied write does not mutate

- **WHEN** a role lacking the write bit sets a property
- **THEN** the call returns a permission-denied error and the property's
  value is unchanged

#### Scenario: Denied wrapped read errors

- **WHEN** a wrapped property handle constructed with a role lacking the
  read bit reads the property
- **THEN** the read returns a permission-denied error instead of a value

#### Scenario: Theme cannot write widget-owned properties

- **WHEN** a theme writes a runtime-computed property whose write mask
  excludes the theme role
- **THEN** the write is denied and the widget's computed value stands

### Requirement: Float-array expression evaluation with dependency re-evaluation

Bounded float-array properties (for example 4-component colors) SHALL
support per-index expressions evaluated against globals provided by the
property's dependencies plus any evaluation extras. Indices holding plain
values SHALL be left untouched by evaluation. When a dependency of such a
property changes, the consuming widget SHALL re-evaluate the affected
indices so that subsequent reads observe the new computed result.

#### Scenario: Dependency change recolors a wired property

- **WHEN** a color property's four indices are expressions referencing a
  token property, and the token's value changes
- **THEN** the color property's subsequent reads yield the color computed
  from the new token value

#### Scenario: Mixed plain and expression indices

- **WHEN** one index of a float-array property holds a plain value and the
  others hold expressions, and evaluation runs
- **THEN** only the expression indices are recomputed and the plain index
  keeps its value
