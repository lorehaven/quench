---
title: AT-SPI coverage
parent: quench-desktop-atspi
grand_parent: Crates
nav_order: 3
---

What a screen reader sees. All of it is answered from the last snapshot given to `publish`.

## Object tree

| Object | Path | Role | Children | Interfaces |
|---|---|---|---|---|
| Application | `/org/a11y/atspi/accessible/root` | 75 (`application`) | the window | `Accessible`, `Application` |
| Window (frame) | `/org/a11y/atspi/accessible/window` | 23 (`frame`) | the page root (the tree's root node) | `Accessible`, `Component` |
| Node | `/org/a11y/atspi/accessible/<id>` | by role (table below) | the node's children | `Accessible`, `Component`, plus `Action`, `Text`, `EditableText`, `Value` when they apply |

The application's parent is the registry's root (set from the `Embed` reply). The window's parent is the application. A node's parent is the node recorded when the snapshot was published, or the window for the root. A path that is not one of these, or a node id not in the snapshot, answers `org.freedesktop.DBus.Error.UnknownObject` (message = the path). The application and window names are the `app_name`.

`org.freedesktop.DBus.Peer`: `Ping` answers an empty reply; every other member answers the constant machine id `0123456789abcdef0123456789abcdef`.

An unknown member answers `org.freedesktop.DBus.Error.UnknownMethod` (`<interface>.<member>`). An unknown property answers `org.freedesktop.DBus.Error.UnknownProperty`.

## Interfaces offered (`GetInterfaces`)

* Always `org.a11y.atspi.Accessible`.
* Application: `org.a11y.atspi.Application`.
* Window: `org.a11y.atspi.Component`.
* Node: `Component`; `Action` if the node has an action other than scroll-into-view; `Text` for `TextBox` and `Text` roles; `EditableText` for a `TextBox` that is neither read-only nor disabled; `Value` if the node has a `range`.

## Properties (`org.freedesktop.DBus.Properties.Get` / `GetAll`)

| Interface | Properties |
|---|---|
| `Accessible` | `Name`, `Description`, `Parent`, `ChildCount`, `Locale` (`en_US`), `AccessibleId` (empty) |
| `Application` | `ToolkitName` (`quench`), `Version` (`0.1`), `Id` (0) |
| `Action` | `NActions` |
| `Text` | `CharacterCount`, `CaretOffset` |
| `Value` | `MinimumValue`, `MaximumValue`, `CurrentValue`, `MinimumIncrement` (`(max - min) / 100`; an indeterminate progress bar reports the minimum as current) |

## Methods

| Interface | Methods |
|---|---|
| `Accessible` | `GetChildAtIndex` (a null reference out of range), `GetChildren`, `GetIndexInParent`, `GetRelationSet` (always empty), `GetRole`, `GetRoleName`, `GetLocalizedRoleName`, `GetState`, `GetAttributes` (`toolkit=quench`, plus `level=<n>` for a heading), `GetApplication`, `GetInterfaces` |
| `Application` | `GetLocale` (`en_US`), `GetApplicationBusAddress` (empty) |
| `Component` | `Contains`, `GetAccessibleAtPoint` (deepest child under the point, last child first; the document root itself is never returned), `GetExtents`, `GetPosition`, `GetSize`, `GetLayer` (3), `GetMDIZOrder` (0), `GetAlpha` (1.0), `GrabFocus` (queues a focus action; answers whether the node is focusable), `ScrollTo` (queues scroll-into-view; answers true) |
| `Action` | `GetNActions`, `GetName`/`GetLocalizedName` (`click`, `focus`), `GetDescription`/`GetKeyBinding` (empty), `GetActions`, `DoAction` (queues the action; false for a bad index) |
| `Text` | `GetText(start, end)` (an end of 0 or less means to the end), `GetCharacterCount`, `GetCaretOffset`, `GetNSelections` (1 if caret and anchor differ), `GetSelection` |
| `EditableText` | `SetTextContents`, `InsertText(position, text)`, `DeleteText(start, end)` (each queues a whole new text for the field); `CopyText`, `CutText`, `PasteText` answer false |

`Text` for a `TextBox` is its value; for a `Text` node it is its name. Offsets are in characters.

Not implemented: `Selection`, `Table`, `Image`, `Document`, `Hypertext`, `Collection`, relations, text attributes, `GetTextAtOffset`, `GetBoundedRanges` and the rest of `Text`, `SetCaretOffset`, `SetSelection`, `GetCharacterExtents`, object-level `Registry` event registration (signals are broadcast without checking who listens).

## Roles

AT-SPI role numbers (`AtspiRole`) and `GetRoleName` strings:

| `Role` | Number | Name |
|---|---|---|
| Document | 95 | document web |
| Heading | 83 | heading |
| Paragraph | 73 | paragraph |
| Text | 116 | static |
| Button | 43 | push button |
| Button with `selected` set | 62 | toggle button |
| Switch | 62 | toggle button |
| Link | 88 | link |
| Checkbox | 7 | check box |
| Radio | 44 | radio button |
| TextBox | 79 | entry |
| List | 31 | list |
| ListItem | 32 | list item |
| Tab | 37 | page tab |
| TabList | 38 | page tab list |
| TabPanel | 39 | panel |
| Dialog | 16 | dialog |
| Navigation | 110 | landmark |
| Status | 54 | status bar |
| Alert | 2 | alert |
| Image | 27 | image |
| Form | 87 | form |
| Group | 85 | section |
| Slider | 51 | (none: `unknown`) |
| ProgressBar | 48 | (none: `unknown`) |
| window | 23 | frame |
| application | 75 | application |

## States (`GetState`, a 64-bit set as two `u32`)

Always set: `ENABLED` (8), `SENSITIVE` (24), `SHOWING` (25), `VISIBLE` (30). The window also has `ACTIVE` (1). For nodes: `disabled` clears `ENABLED` and `SENSITIVE`; `focusable` -> `FOCUSABLE` (11); `focused` -> `FOCUSED` (12); `checked` present -> `CHECKABLE` (41), true -> `CHECKED` (4); `selected` present: on a `Tab` -> `SELECTABLE` (22) and `SELECTED` (23) if true, otherwise `PRESSED` (20) if true; `expanded` present -> `EXPANDABLE` (9), true -> `EXPANDED` (10); a `TextBox` -> `SINGLE_LINE` (26) and `READ_ONLY` (43) or `EDITABLE` (7); `required` -> `REQUIRED` (33).

## Coordinates

`GetExtents`, `GetPosition`, `GetSize`, `Contains` and `GetAccessibleAtPoint` use the node's `bounds` (x, y, w, h) as the renderer reports them, truncated to integers, in the window's coordinate space. The coordinate-type argument of the AT-SPI methods is ignored.
