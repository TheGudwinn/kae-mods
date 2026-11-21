Content for the app
===================

Kae Mods reads two files from this folder (from the `main` branch) and shows them inside the app.
Both are lists. Empty lists are fine: the app then shows a short "nothing here yet" note.

`news.json`
-----------

```json
[
  {
    "id": "2026-11-patch",
    "date": "2026-11-21",
    "title": "Short headline",
    "text": "One or two sentences.",
    "tag": "Patch",
    "url": "https://example.com/full-story"
  }
]
```

`tag` and `url` are optional. `url` must start with `https://`.

`collections.json`
------------------

A collection is a profile code (Profiles page, "Copy code") with a name and a few words.

```json
[
  {
    "id": "low-fps",
    "name": "Minimal FPS drop",
    "author": "name",
    "description": "Small mods that do not touch heroes.",
    "code": "kae1:..."
  }
]
```

To suggest a collection, use "Suggest yours" on the Profiles page: it opens an issue with the code filled in.
