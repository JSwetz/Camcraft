{
  "general": { "model": "Tormach 770MX", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -13.25, "max": 0.0 }, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -3.75, "max": 3.75 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -7.0, "max": 7.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 175.0, "max_speed": 10000.0, "power": 1.1 },
          "tool_station": {
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 300.0, "max_rapid_speed": 300.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 300.0, "max_rapid_speed": 300.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 230.0, "max_rapid_speed": 230.0 }
  } } },
  "tooling": { "number_of_tools": 10 }
}
