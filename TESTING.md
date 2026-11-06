# CamCraft 0.7.0 - hand-test checklist

These are about 12 checks, roughly 30 minutes in all, aimed at what changed
since 0.6.0. For each one, do the steps and compare what you see with
**Expect**. Anything that differs is worth a note: a screenshot and one
sentence is plenty.

Start `camcraft-ui.exe`. Import a part (Model), set Stock, and add a tool
(Tools).

1. **Source picker.** Create a Pocket and open the Source dropdown.
   **Expect:** only *Stock* and your models are listed; no G54-G59, setups or
   tools.

2. **The dialog.** On any operation, look at Heights and at Lead in / Lead out.
   **Expect:** label and value on one row, units beside the value, collapsible
   groups, a status line at the top, Generate / Apply / Clear / Delete in a bar
   at the bottom.

3. **Fields that apply.** In Lead in / Lead out, switch Entry between Helix,
   Ramp and Plunge. **Expect:** helix settings show only for Helix, ramp
   settings only for Ramp.

4. **Auto toggles.** Find a field with an *Auto* checkbox (e.g. a stepover or
   plunge feed). Turn Auto off. **Expect:** a sensible value appears and becomes
   editable; Auto on greys it out again.

5. **Boundary sentence.** On Face, pick an open boundary edge. **Expect:** one
   plain sentence explains what will be machined; *Machine the other side* flips
   it, and the viewport shading moves with it.

6. **Open-boundary Contour.** Contour with an open boundary along a part edge.
   **Expect:** one open pass along that edge on the chosen side, entering and
   leaving past the ends.

7. **Adaptive.** A 2D Adaptive pocket in a rectangle. **Expect:** a smooth
   spiral, one entry per level, no plunge straight into the material before the
   entry.

8. **Simulation.** Generate a longer program and press Simulate; scrub the
   playhead back and forth. **Expect:** no lock-up; the stock is cut as the
   cutter passes; jumping far is allowed to take a few seconds, but the window
   stays responsive.

9. **Clear vs delete.** Generate an operation, then use *Clear toolpath*.
   **Expect:** the path is gone and the operation stays. Errors shown on the
   page clear when you fix the input.

10. **Vertex and face origin.** Geometry tab → Click selects: *Vertices*. Hover
    a corner, click it, then *Set origin*. Do the same on a top face with *Set
    origin (Z only)*. **Expect:** G54 moves to the corner / the face height;
    Ctrl+Z undoes it.

11. **Pocket arc entry.** In a Pocket's Lead in / Lead out, turn on *Horizontal
    arc entry*. **Expect:** after the descent, the tool arcs flat onto the first
    pass. In a zigzag pocket the notes say why it fell back.

12. **Panels.** Close the browser and properties panels. **Expect:** *Panels…*
    brings them back; *Reset layout* restores the default.

If something goes wrong (a crash, a toolpath that looks like it cuts the part,
or a warning you think is wrong), note the operation, the step, and the status
line text.
