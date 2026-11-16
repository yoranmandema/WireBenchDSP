// The left rack: the block catalogue, grouped by category.

describe("block rack", () => {
  beforeEach(() => {
    cy.boot();
  });

  it("groups the catalogue by category in display order", () => {
    cy.get(".block-list h2")
      .then(($h) => [...$h].map((el) => el.textContent.trim()))
      .should("deep.equal", ["Modelling", "Distortion", "Tone", "Time", "Pitch", "Utility"]);
  });

  it("adds a block to the first channel when its row is clicked", () => {
    cy.get(".channel").eq(0).find(".block-card").should("have.length", 3);

    cy.contains(".rack-item", /^Delay$/).click();

    cy.get(".channel").eq(0).find(".block-card").should("have.length", 4);
    cy.get(".channel").eq(0).find(".block-card").eq(3).should("contain", "Delay");
  });

  it("offers every registry block exactly once", () => {
    cy.get(".rack-item").then(($items) => {
      const names = [...$items].map((el) => el.textContent.trim());
      expect(names).to.have.length(11);
      expect(new Set(names).size).to.equal(names.length);
    });
  });
});
