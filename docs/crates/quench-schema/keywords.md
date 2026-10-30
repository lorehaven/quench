---
title: Keyword reference
parent: quench-schema
grand_parent: Crates
nav_order: 4
---

The schema "language" is JSON Schema; this page lists exactly which keywords are evaluated and the failure each produces (messages and `ErrorKind` values were produced by running the keyword; the instance is shown as compact JSON in messages). A schema is an object or a boolean: `true` accepts everything, `false` rejects everything (`False schema does not allow 1`, kind `False`). Any other schema type is a `SchemaError` (`a schema is an object or a boolean`).

All keywords of a schema object are evaluated and all problems are reported, in this order: `type`, `enum`, `const`, then string/number keywords, array keywords, object keywords, combinators, and finally `$ref`.

## Type keywords

| Keyword | Value | Fails when | Message / kind |
|---|---|---|---|
| `type` | a name or an array of names: `null`, `boolean`, `integer`, `number`, `string`, `array`, `object` | none of the names matches | `1 is not of type "string"`; with several: `1 is not of types "null", "string"` (names sorted in the order null, boolean, integer, number, string, array, object; unknown names last). Kind `Type { expected }`. |
| `enum` | array | no element is equal to the instance | `"c" is not one of "a" or "b"`; with 3 options `"a", "b" or "c"`; with more, `"z" is not one of "a", "b" or 3 other candidates`. Kind `Enum`. |
| `const` | any | not equal | `5 was expected`. Kind `Const`. |

Equality is JSON equality in which `1` and `1.0` are the same number; objects compare by keys and values regardless of order. `integer` accepts floats with no fractional part (`1.0`). `number` accepts any number. An unknown type name (`"type":"unknown"`) is accepted in the schema and simply never matches. A `type` that is neither a string nor an array of strings is a `SchemaError`.

## String keywords (apply only to string instances)

| Keyword | Fails when | Message / kind |
|---|---|---|
| `minLength` (non-negative integer) | fewer characters (Unicode scalar values) | `"ab" is shorter than 3 characters` (`1 character`); `MinLength { limit }` |
| `maxLength` | more characters | `"ab" is longer than 1 character`; `MaxLength { limit }` |
| `pattern` (string) | the regular expression does not match **anywhere** in the string (unanchored) | `"AB" does not match "^[a-z]+$"`; `Pattern { pattern }` |

Patterns use [quench-regex](../quench-regex/index.md) syntax (see its limits: no look-around, no backreferences, no `\p{..}`). `format` is an annotation: it never fails.

## Number keywords (apply only to number instances)

| Keyword | Fails when | Message / kind |
|---|---|---|
| `minimum` | `instance < limit` | `0 is less than the minimum of 1`; `Minimum { limit: f64 }` |
| `maximum` | `instance > limit` | `11 is greater than the maximum of 10`; `Maximum` |
| `exclusiveMinimum` | `instance <= limit` | `1 is less than or equal to the minimum of 1`; `ExclusiveMinimum` |
| `exclusiveMaximum` | `instance >= limit` | `1 is greater than or equal to the maximum of 1`; `ExclusiveMaximum` |
| `multipleOf` (> 0; a value <= 0 is ignored) | not a multiple | `4 is not a multiple of 3`; `MultipleOf { divisor }` |

Limits print as written in the schema (`5` stays `5`). Comparisons are done in `f64`. Integer instance and integer divisor use exact integer remainder; otherwise a quotient within `1e-9` (relative) of an integer counts as a multiple, so `0.3` is a multiple of `0.1`. A boolean or string instance is not a number (the keywords do nothing).

## Array keywords (apply only to arrays)

| Keyword | Fails when | Message / kind |
|---|---|---|
| `minItems` | fewer items | `[1] has less than 2 items` (`1 item`); `MinItems { limit }` |
| `maxItems` | more items | `[1,2] has more than 1 item`; `MaxItems { limit }` |
| `uniqueItems: true` | two equal items | `[1,1.0] has non-unique elements`; `UniqueItems` |
| `contains` (schema) | no item is valid under it | `None of [1,2] are valid under the given schema`; `Contains` |
| `prefixItems` (array of schemas) | item `i` is checked against schema `i` for the leading items | errors of the subschema, path `/i` |
| `items` (schema) | applies to the items **after** the `prefixItems` | errors of the subschema, path `/i` |

`minContains`/`maxContains`, `unevaluatedItems` are not supported (ignored). The draft-07 array form of `items` (`"items": [ ... ]`) is rejected by `Schema::new` with `a schema is an object or a boolean`; use `prefixItems`.

## Object keywords (apply only to objects)

| Keyword | Fails when | Message / kind |
|---|---|---|
| `minProperties` | fewer entries | `{} has less than 1 property` (`2 properties`); `MinProperties` |
| `maxProperties` | more entries | `{"a":1,"b":2} has more than 1 property`; `MaxProperties` |
| `required` (array of strings) | a name is absent | one error per missing name: `"a" is a required property`; `Required { property }` |
| `dependentRequired` (object name -> names) | the trigger is present and a listed name is absent | `"b" is a required property`; `Required { property }` |
| `propertyNames` (schema) | a key is invalid under it | the subschema's message for the key string, path = the object's path, kind `PropertyNames` |
| `properties` (object name -> schema) | a present property fails its schema | errors of the subschema, path `/name` |
| `patternProperties` (object pattern -> schema) | a key matching the (unanchored) pattern fails the schema | errors of the subschema, path `/key` |
| `additionalProperties` (schema or boolean) | see below | |

`additionalProperties` applies to keys that are in neither `properties` nor matched by any `patternProperties` pattern. With `false`: one error listing all of them: `Additional properties are not allowed ('b', 'c' were unexpected)` (`'b' was unexpected` for one); kind `AdditionalProperties { unexpected }`. With `true`: nothing. With a schema: each such value is validated against it, path `/key`.

Not supported: `dependentSchemas`, `unevaluatedProperties`, `dependencies`.

## Combinators

| Keyword | Rule | Message / kind |
|---|---|---|
| `allOf` | every branch is applied (errors of all branches are reported directly, not wrapped) | the branches' own errors |
| `anyOf` | at least one branch valid | `null is not valid under any of the schemas listed in the 'anyOf' keyword`; `AnyOf` |
| `oneOf` | exactly one branch valid | none: `... 'oneOf' keyword` (`OneOfNotValid`); several: `2 is valid under more than one of the schemas listed in the 'oneOf' keyword` (`OneOfMultipleValid`) |
| `not` | the instance must not be valid | `{"type":"integer"} is not allowed for 1`; `Not` |
| `if` / `then` / `else` | if `if` is valid apply `then`, else apply `else` (each optional) | the applied branch's own errors |

The errors of `anyOf`, `oneOf`, `not` and `if` conditions are discarded; only the summary is reported.

## `$ref`

A reference is resolved against the **root** schema as a JSON Pointer after `#`: `#` is the root, `#/$defs/x` and `#/definitions/x` (any pointer, in fact). The referenced schema is applied to the same instance, with its errors reported in place; other keywords next to `$ref` are evaluated too. `$defs` and `definitions` are walked by `Schema::new` to compile their patterns and check their shape.

**An unresolvable reference is silently ignored** (`{"$ref": "#/nope"}` accepts everything), as are references that are not local (`https://...`). `$id`, `$anchor`, `$dynamicRef` and `$schema` are not interpreted. A reference cycle stops after 128 nested levels and then accepts.

## Annotations (never fail)

`format`, `title`, `description`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly`, `$schema`, `$id`, `$comment`, `x-kubernetes-*`, and every other unknown keyword.

## Shape checks in `Schema::new`

| Keyword | Required shape | Error |
|---|---|---|
| `pattern` | string, valid regex | `` `pattern` is a string `` / `` `(` is not a valid pattern: regex parse error at 1: unclosed group `` |
| `type` | string or array of strings | `` `type` is a string or an array of strings `` |
| `required` | array of strings | `` `required` is an array of strings `` |
| `enum` | array | `` `enum` is an array `` |
| `properties`, `patternProperties`, `dependentRequired`, `$defs`, `definitions` | object | `` `properties` is an object `` |
| `allOf`, `anyOf`, `oneOf`, `prefixItems` | array of schemas | `` `allOf` is an array of schemas `` |
| `items`, `additionalProperties`, `not`, `contains`, `propertyNames`, `if`, `then`, `else` | a schema | `a schema is an object or a boolean` |
| `minLength`, `maxLength`, `minItems`, `maxItems`, `minProperties`, `maxProperties` | non-negative integer | `` `minLength` is a non-negative integer `` |
| `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` | number | `` `minimum` is a number `` |
| nesting | at most 128 deep | `nested too deeply` |

All errors are prefixed `invalid schema: ` by `Display`. `const` is not shape-checked; keywords not listed in the table are not checked.

## Instance paths

`ValidationError::instance_path()` is a JSON Pointer into the instance: `""` for the whole instance, `/spec/replicas`, `/items/2`; `~` is written `~0` and `/` is written `~1` (`/a~1b`). For `required`, `additionalProperties`, `propertyNames` and similar keyword errors the path is the **object**, not the missing property.
