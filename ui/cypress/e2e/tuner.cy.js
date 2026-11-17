// Tuner top-bar button and modal: open/close, note readout, cents needle,
// verdict, frequency line, signal bar, idle state, keyboard and viewport geometry.
describe("tuner", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("shows the note, octave, verdict, needle, frequency and signal while active", () => {
    // The trigger is the first item in the top-right area of the top bar.
    cy.get('button[title="Tuner (T)"]').should("exist");

    // Open the modal.
    cy.get('button[title="Tuner (T)"], button[aria-label="Open the tuner"]').click();

    // The modal is visible and the trigger carries the "on" class.
    cy.get(".modal.tuner").should("be.visible");
    cy.get('button[title="Tuner (T)"], button[aria-label="Open the tuner"]').should(
      "have.class",
      "on"
    );

    // Note readout: the event alone drives it (no tuner block in the rack).
    cy.get(".tuner-note").contains("A");
    cy.get(".tuner-note").contains("3");

    // Verdict: |cents| = 1.2 <= 5  => "In tune".
    cy.get(".tuner-verdict").should("contain", "In tune");

    // Needle within 10% of centre (cents = -1.2 => left ≈ 48.8%).
    cy.get(".tuner-needle").then(($el) => {
      const left = parseFloat(getComputedStyle($el[0]).left);
      expect(left).to.be.within(40, 60);
    });

    // Frequency line and signal bar are present.
    cy.get(".tuner-freq").should("contain", "220.0 Hz");
    cy.get(".tuner-freq").should("contain", "A4 = 440 Hz");
    cy.get(".tuner-freq").should("be.visible");
    cy.get(".signal").should("be.visible");
  });

  it("closes on Escape and reopens on t", () => {
    cy.get('button[title="Tuner (T)"]').click();
    cy.get(".modal.tuner").should("be.visible");

    cy.window().then((win) => {
      win.dispatchEvent(
        new win.KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })
      );
    });

    cy.get(".modal.tuner").should("not.exist");

    // "t" reopens it.
    cy.window().then((win) => {
      win.dispatchEvent(
        new win.KeyboardEvent("keydown", { key: "t", bubbles: true, cancelable: true })
      );
    });

    cy.get(".modal.tuner").should("be.visible");
  });

  it("moves the needle to the left edge and shows flat for a -48.5 cent reading", () => {
    cy.get('button[title="Tuner (T)"]').click();
    cy.get(".modal.tuner").should("be.visible");

    cy.request("POST", "http://127.0.0.1:9400/__tuner", {
      active: true,
      freq: 207.0,
      note: "A",
      octave: 3,
      cents: -48.5,
      clarity: 0.8,
      reference: 440,
    }).then(() => {
      // Needle near the left edge (cents = -48.5 => left ≈ 1.5%).
      cy.get(".tuner-needle").then(($el) => {
        const left = parseFloat(getComputedStyle($el[0]).left);
        expect(left).to.be.below(10);
      });

      // Verdict: flat.
      cy.get(".tuner-verdict").should("contain", "flat");
    });
  });

  it("shows the idle state when inactive", () => {
    cy.get('button[title="Tuner (T)"]').click();
    cy.get(".modal.tuner").should("be.visible");

    cy.request("POST", "http://127.0.0.1:9400/__tuner", { active: false }).then(() => {
      cy.get(".tuner-idle").should("be.visible");
      cy.get(".tuner-note").should("have.text", "—");
      cy.get(".tuner-verdict").should("not.exist");
      cy.get(".tuner-gauge").should("not.exist");
      cy.get(".tuner-freq").should("not.exist");
      cy.get(".signal").should("not.exist");
    });
  });

  it("stays inside the viewport at 800x600 and 1440x900", () => {
    cy.get('button[title="Tuner (T)"]').click();

    cy.viewport(800, 600);
    cy.get(".modal.tuner").then(($el) => {
      const r = $el[0].getBoundingClientRect();
      expect(r.left).to.be.at.greaterThanOrEqual(0);
      expect(r.top).to.be.at.greaterThanOrEqual(0);
      expect(r.right).to.be.at.lessThanOrEqual(800);
      expect(r.bottom).to.be.at.lessThanOrEqual(600);
    });

    cy.viewport(1440, 900);
    cy.get(".modal.tuner").then(($el) => {
      const r = $el[0].getBoundingClientRect();
      expect(r.left).to.be.at.greaterThanOrEqual(0);
      expect(r.top).to.be.at.greaterThanOrEqual(0);
      expect(r.right).to.be.at.lessThanOrEqual(1440);
      expect(r.bottom).to.be.at.lessThanOrEqual(900);
    });
  });
});
