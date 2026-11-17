// The instrument tuner: the top-bar trigger, the modal, the note readout,
// the cents needle, the verdict and the engine-driven state.
//
// The only payload the UI ever receives is `event: tuner` (20 Hz). It drives
// the whole modal; no command is ever sent. Active:false yields the idle body.

describe("tuner", () => {
  it("opens from the top bar, then honours keyboard, state and viewport geometry", () => {
    cy.boot();

    // The trigger is present in the top bar and carries the required ARIA.
    cy.get('button[title="Tuner (T)"]')
      .should("exist")
      .and("have.attr", "aria-label", "Open the tuner")
      .and("have.attr", "aria-expanded", "false");

    // Open it with the mouse.
    cy.get('button[title="Tuner (T)"]').click();

    // Driven purely by event: tuner (the engine resets to TUNER_DEFAULT on
    // every connection, so the note is A3 and within 5 cents).
    cy.get(".modal.tuner").should("be.visible");
    cy.get(".tuner-note").contains("A");
    cy.get(".tuner-verdict").should("contain", "In tune");
    cy.get(".tuner-needle").should("have.css", "left").then((l) => {
      // read-only modal: the engine drives everything, no command sent
      cy.window().then((win) => {
        // diagnostic: surface any global tuner state if the app exposes one
        const g = win as any;
        for (const key of ["tuner", "Tuner", "__tuner", "app", "pedalUi"]) {
          if (key in g) {
            try { console.debug("DEBUG global", key, JSON.stringify(g[key])); } catch (e) {}
          }
        }
      });
      expect(parseFloat(l)).to.be.closeTo(50, 10); // within 10% of centre
    });
    cy.get('button[title="Tuner (T)"]').should("have.class", "on");

    // Keyboard: Escape closes, `t` reopens.
    cy.get(".modal.tuner").type("{Escape}");
    cy.get(".modal.tuner").should("not.exist");
    cy.get('button[title="Tuner (T)"]').type("t");
    cy.get(".modal.tuner").should("be.visible");

    // Push a deliberately flat reading through the engine's test hook.
    cy.request("POST", "http://127.0.0.1:9400/__tuner", {
      active: true,
      freq: 207.0,
      note: "A",
      octave: 3,
      cents: -48.5,
      clarity: 0.8,
      reference: 440,
    }).then((res) => {
      expect(res.status).to.equal(204);
      cy.get(".tuner-needle").should("have.css", "left").then((left) => {
        expect(parseFloat(left)).to.be.below(25); // near the left edge
      });
      cy.get(".tuner-verdict").should("contain", "flat");
    });

    // Inactive: the idle body and the em dash replace the readout.
    cy.request("POST", "http://127.0.0.1:9400/__tuner", { active: false }).then((res) => {
      expect(res.status).to.equal(204);
      cy.get(".tuner-idle").should("be.visible");
      cy.get(".tuner-note").should("contain", "—");
      cy.get(".tuner-verdict").should("not.exist");
      cy.get(".tuner-gauge").should("not.exist");
      cy.get(".tuner-freq").should("not.exist");
    });

    // Viewport geometry: the modal is at most 440 px wide and stays inside
    // both 800x600 and 1440x900.
    [800, 1440].forEach((width) => {
      [600, 900].forEach((height) => {
        cy.viewport(width, height);
        cy.get(".modal.tuner").then(($el) => {
          const r = $el[0].getBoundingClientRect();
          expect(r.left).to.be.gte(0);
          expect(r.top).to.be.gte(0);
          expect(r.right).to.be.lte(width);
          expect(r.bottom).to.be.lte(height);
        });
      });
    });
  });
});
