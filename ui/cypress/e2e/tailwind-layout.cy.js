// Visual contracts for the Tailwind migration: key layout and overlay geometry.
describe("Tailwind layout contracts", () => {
  beforeEach(() => cy.boot());

  it("fills the desktop viewport and keeps the editor bar at its design height", () => {
    cy.viewport(1440, 900);
    cy.get(".app").should(($app) => {
      expect($app[0].getBoundingClientRect().height).to.be.closeTo(900, 1);
    });
    cy.get(".editor").should(($editor) => {
      expect($editor[0].getBoundingClientRect().height).to.be.closeTo(224, 1);
    });
  });

  it("keeps the complete app inside a short tablet viewport", () => {
    cy.viewport(800, 600);
    cy.get(".app").should(($app) => {
      expect($app[0].getBoundingClientRect().height).to.be.closeTo(600, 1);
      expect($app[0].scrollHeight).to.be.at.most(600);
    });
    cy.get(".editor").should("be.visible");
  });

  it("renders the add-block picker as a fixed, centered modal", () => {
    cy.get(".channel").first().find(".add-block").click();
    cy.get(".modalBackdrop").should(($backdrop) => {
      expect(getComputedStyle($backdrop[0]).position).to.equal("fixed");
      expect(getComputedStyle($backdrop[0]).display).to.equal("grid");
    });
    cy.get(".modalShell").should(($shell) => {
      expect(getComputedStyle($shell[0]).display).to.equal("flex");
      expect(getComputedStyle($shell[0]).flexDirection).to.equal("column");
    });
  });

  it("renders a shaded knob face and a visible indicator tick", () => {
    cy.get(".block-card").first().click();
    cy.get(".knob-unit").first().find(".knob").should(($knob) => {
      expect(getComputedStyle($knob[0]).backgroundImage).to.contain("radial-gradient");
      const indicator = $knob[0].querySelector(".knob-indicator");
      expect(indicator).not.to.equal(null);
      expect(getComputedStyle(indicator).backgroundColor).not.to.equal("rgba(0, 0, 0, 0)");
    });
  });
});
