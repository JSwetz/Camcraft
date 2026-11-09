{
  "general": { "model": "VMC 3-AXIS", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -500.0, "max": 500.0 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -500.0, "max": 500.0 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -600.0, "max": 600.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 50.0, "max_speed": 12000.0, "power": 7.5,
                       "body_diameter": 80.0, "body_length": 60.0 },
          "tool_station": { "max_tool_length": 300.0, "max_tool_diameter": 100.0,
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 5000.0, "max_rapid_speed": 6000.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 5000.0, "max_rapid_speed": 6000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 3000.0, "max_rapid_speed": 4000.0 }
  } } },
  "tooling": { "number_of_tools": 20 }
}
