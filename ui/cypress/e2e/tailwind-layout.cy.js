// Visual contracts for the Tailwind migration: key layout and overlay geometry.
describe("Tailwind layout contracts", () => {
  beforeEach(() => cy.boot());

  it("fills the desktop viewport and keeps the editor bar at its design height", () => {
    cy.viewport(1440, 900);
    cy.get(".app").should(($app) => {
      expect($app[0].getBoundingClientRect().height).to.be.closeTo(900, 1);
    });
    // `--ui-bar` in ui/src/tailwind.css: the bar's fixed desktop height.
    cy.get(".editor").should(($editor) => {
      expect($editor[0].getBoundingClientRect().height).to.be.closeTo(128, 1);
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
    cy.get(".knob").should(($knob) => {
      expect(getComputedStyle($knob[0]).backgroundImage).to.contain("radial-gradient");
      const indicator = $knob[0].querySelector(".knob-indicator");
      expect(indicator).not.to.equal(null);
      expect(getComputedStyle(indicator).backgroundColor).not.to.equal("rgba(0, 0, 0, 0)");
    });
  });

  // The bottom bar's spacing contracts, checked at the two ends of the range:
  // the page never scrolls sideways, the identity column never overflows its
  // track, every control sits inside the bar, and every target is >= 44px.
  const barContracts = () => {
    cy.document().then((doc) => {
      expect(doc.documentElement.scrollWidth).to.be.at.most(doc.documentElement.clientWidth);
    });
    cy.get(".editor").then(($bar) => {
      const bar = $bar[0].getBoundingClientRect();
      const controls = $bar[0].querySelector(".editor-controls").getBoundingClientRect();
      expect(controls.bottom).to.be.at.most(bar.bottom + 0.5);
    });
    cy.get(".editor-id").then(($id) => {
      expect($id[0].scrollWidth).to.be.at.most($id[0].clientWidth);
    });
    cy.get(".side-btn, .seg, .select").then(($targets) => {
      for (const el of $targets) {
        const r = el.getBoundingClientRect();
        expect(Math.min(r.width, r.height)).to.be.at.least(44);
      }
    });
  };

  it("keeps the bottom bar inside the window at phone and desktop widths", () => {
    cy.selectBlock(0, 0);
    cy.viewport(360, 800);
    barContracts();
    cy.viewport(1440, 900);
    barContracts();
  });

  it("lifts the toast clear of the bar at phone and desktop widths", () => {
    // The seeded `locked` block always refuses `removeBlock`.
    const reject = () => {
      cy.get(".channel").eq(0).find(".block-card").eq(2).find(".remove").click();
      cy.get(".toast").should("contain", "no such block");
    };
    const gapAboveBar = () => {
      cy.get(".toast").then(($t) => {
        const bar = Cypress.$(".editor")[0].getBoundingClientRect();
        const toast = $t[0].getBoundingClientRect();
        expect(toast.bottom).to.be.at.most(bar.top);
        expect(bar.top - toast.bottom).to.be.closeTo(8, 1);
      });
      cy.get(".toast").click();
      cy.get(".toast").should("not.exist");
    };

    cy.viewport(360, 800);
    reject();
    gapAboveBar();
    cy.viewport(1440, 900);
    reject();
    gapAboveBar();
  });
});
