// Submit as an MCP author source body with namespace: "badge-demo".
// Constructors initialize once. Explicit setters are for subsequent revisions.
const info = {width:960,height:540,rate:[24,1],frames:240};
const motion = plan.document('motion','fold.motion.document','Badge Wave — MoGraph');
motion.initialize(d=>d.info(info));
motion.object('background','fold.motion.rectangle','Background').initialize(n=>{
  n.set('width',960).set('height',540);
  n.role('transform',{translation:{Value:datum([0,0])}});
  n.role('appearance',{color:{Value:datum([0.25,0.48,0.27,1])}});
});
const badge = motion.badge('badge',{text:'Hi Fold',size:24,padding:[24,12],radius:24,position:[715,270]});
motion.style('outline',badge.background,'fold.motion.stroke',{width:1.5,color:[0.018,0.012,0.065,1]});
const copies = motion.duplicate('copies',badge,{name:'Duplicator',count:52,spacing:[-9,0],stagger:0.024});
motion.oscillator('bounce',copies,'offset_y',{minimum:-60,maximum:60,duration:1.25});
motion.colorRamp('palette',copies,'color',[[0,[0.38,0.025,0.12,1]],[1,[0.1,0.12,0.85,1]]]);
const composite=plan.composite('composite',motion,info,'Badge Wave — Composite');
plan.sequence('sequence',composite,'Badge Wave — Sequence');
