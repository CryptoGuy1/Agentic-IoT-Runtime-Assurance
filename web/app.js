'use strict';
let state, busy = false, failed = false, selected = 'fan';
const $ = id => document.getElementById(id);
const labels = {Valid:'Supported',Unknown:'Unknown',Invalid:'Contradicted'};
const states = {Pending:'Waiting',Ready:'Ready',Leased:'Permission issued',Executing:'Running',Committed:'Committed',Aborting:'Stopping requested',Recovering:'Recovering',Completed:'Completed',Failed:'Failed',Cancelled:'Cancelled'};
const names = {s1:'Hazard sensor 1',s2:'Hazard sensor 2',thermal:'Thermal sensor',pressure_safe:'Pressure acceptable',fan_healthy:'Fan healthy',occupancy_clear:'Room empty',egress_clear:'Exit clear',agent_available:'Planner available'};
const sensorHelp = {
 s1:'Hazard confirmation needs any 2 of the 3 hazard sensors.',
 s2:'Hazard confirmation needs any 2 of the 3 hazard sensors.',
 thermal:'Confirms a hazard; this model has no temperature in degrees. Any 2 hazard sensors suffice.',
 pressure_safe:'Required while the fan action runs. Losing it requests a stop.',
 fan_healthy:'Required to start and run the fan.',
 occupancy_clear:'Used by the physical safety check before the door closes.',
 egress_clear:'Used by door safety checks and required while the door action runs.',
 agent_available:'Required by action safety checks. Loss can prevent or interrupt actions.',
};
const esc = v => String(v).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function availability(){document.querySelectorAll('button,select,textarea').forEach(e=>{e.disabled=busy || (failed && e.id!=='reset');}); $('generate-plan').disabled=busy || failed || state?.planner_mode!=='ai';}
async function request(control, preset, mode){
 if(busy)return; busy=true; availability(); $('error').hidden=true; $('planning-status').textContent=control==='generate_plan'?'Qwen is generating locally…':'';
 try{
  const payload=control==='generate_plan'?{control,goal:$('goal').value}:control==='reset'?{control,preset:preset||state?.preset||'pressure',planner_mode:mode||state?.planner_mode||'scripted'}:{control};
  const options=control?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(payload)}:{};
  const response=await fetch(control?'/api/control':'/api/state',options);
  const data=await response.json();
  if(data.state){state=data.state;render();}
  if(!response.ok || data.ok===false){ failed=!data.recoverable; $('error').textContent=(data.error||'Request failed')+(failed?' Use Reset to recover.':' Current plan is unchanged. You can retry.'); $('error').hidden=false; return; }
  failed=false;
 }catch(e){failed=true;$('error').textContent=e.message+' Use Reset to recover.';$('error').hidden=false;}
 finally{busy=false;$('planning-status').textContent='';availability();}
}
function render(){
 $('preset').value=state.preset;
 $('planner-mode').value=state.planner_mode||'scripted';
 $('planner-label').textContent=state.planner_mode==='ai'?'AI planner · Qwen3 1.7B':'Scripted planner';
 const p=state.planner;
 $('proposal').innerHTML=p?`<strong>${esc({accepted:'Plan accepted · execution still needs runtime permission',waiting:'Planner chose to wait · current plan retained',rejected:'Proposal rejected · current plan retained'}[p.decision]||p.decision)}</strong><p>${p.actions.length?esc(p.actions.join(' → ')):'No new actions'}</p><p><b>${p.decision==='rejected'?'Validation result':'Qwen explanation'}:</b> ${esc(p.explanation)}</p><small>${esc(p.model)}${p.latency_ms==null?'':' · '+(p.latency_ms/1000).toFixed(2)+' seconds inference'} · ${esc(p.proposal_id)}</small>`:state.planner_mode==='ai'?'No AI proposal yet. Generate a plan, then advance simulated time.':'The scripted reference supplies the preset’s action sequence.';

 $('clock').innerHTML=(state.time_ms/1000).toFixed(3)+'<span>s</span>';
 const t=state.plant;
 $('fan').classList.toggle('moving',t.fan_high||t.fan_starting);
 $('fan-label').textContent=t.fan_high?'Fan running':t.fan_starting?'Fan starting':'Fan idle';
 $('alarm').classList.toggle('active',t.alarm_active);
 $('door').classList.toggle('closed',t.door_closed);
 $('door-label').textContent=t.door_closed?'Door closed':t.door_closing?'Door closing':'Door open';
 $('hazard-value').textContent=t.hazard?'Present':'Absent'; $('egress-value').textContent=t.egress_clear?'Clear':'Blocked';
 $('pressure-value').textContent=t.pressure+' model units'; $('occupancy').textContent=t.occupied?'Occupied':'Empty';
 $('actions').innerHTML=state.actions.filter(a=>a.current!==false).map(a=>`<button class="action ${a.action===selected?'selected':''}" data-action="${esc(a.action)}"><div><strong>${esc(a.action[0].toUpperCase()+a.action.slice(1))}</strong><small>${esc(a.id)}</small></div><span class="badge ${['Completed','Executing'].includes(a.state)?'valid':['Aborting','Cancelled','Failed'].includes(a.state)?'unknown':''}">${esc(states[a.state]||a.state)}</span></button>`).join('');
 document.querySelectorAll('[data-action]').forEach(b=>b.onclick=()=>{selected=b.dataset.action;render();availability();});
 const action=state.actions.find(a=>a.action===selected && a.current!==false);
 $('selected-title').textContent=selected[0].toUpperCase()+selected.slice(1)+' requirements';
 $('phases').innerHTML=action?action.phases.map(p=>`<div class="phase"><b>${esc(p.phase)}</b><span class="${p.status.toLowerCase()}">${esc(labels[p.status])}</span><small>${p.witness.length?'Supported by '+p.witness.map(id=>esc(names[id]||id)).join(', '):'No sufficient supporting witness'}${p.horizon_ms===null?'':' · until '+(p.horizon_ms/1000).toFixed(3)+'s'}</small></div>`).join(''):'';
 $('evidence').innerHTML=Object.entries(names).map(([id,name])=>{
  const e=state.evidence.find(e=>e.id===id);
  const reading=e?`<span class="${e.status.toLowerCase()}">${esc(labels[e.status])}</span><small>Version ${e.version} · observed ${(e.observed_ms/1000).toFixed(2)}s<br>Expires ${(e.expires_ms/1000).toFixed(2)}s</small>`:'<span class="unknown">Unknown</span><small>No observation delivered yet</small>';
  return `<div class="evidence-card"><strong>${esc(name)}</strong>${reading}<p>${esc(sensorHelp[id])}</p><div class="controls"><button data-sensor-control="${id}_drop" aria-label="Remove ${esc(name)} evidence">Remove</button><button data-sensor-control="${id}_restore" aria-label="Restore ${esc(name)} evidence">Restore</button></div></div>`;
 }).join('');
 document.querySelectorAll('[data-sensor-control]').forEach(b=>b.onclick=()=>request(b.dataset.sensorControl));
 $('events').innerHTML=[...state.events].reverse().map(e=>`<div class="event"><span>${(e.time_ms/1000).toFixed(3)}s · ${esc(e.kind)}</span><p>${esc(e.detail)}</p></div>`).join('');
 guide();
}
function guide(){
 const fan=state.actions.find(a=>a.action==='fan' && a.current!==false);let title,text,control,label;
 if(state.planner_mode==='ai'){
  title='The AI proposes; DCRA controls execution.';text='Generate a plan from the delivered observations, then advance time. Replan after changing evidence to explore how Qwen responds. Accepted plans may still wait for required evidence.';control='advance_500';label='Advance 500 ms →';
 }else if(state.preset==='late'){
  title=state.time_ms<1200?'A permission has an expiry time.':'The permission arrived too late.';
  text=state.time_ms<1200?'The network delays permission longer than its one-second limit. Advance time to see the gateway reject it.':'No action has executed. A renewed permission may still be travelling; permission issued is different from running.';
  control=state.time_ms<1200?'advance_1200':'reset';label=state.time_ms<1200?'Show delayed permission →':'Restart demonstration ↺';
 }else if(fan?.state==='Aborting'||fan?.state==='Recovering'){
  title='A stop is requested, not yet confirmed.';text='Required support has been lost. The runtime is waiting for the simulated controller to respond. Check the selected action’s requirements and observation cards.';control='advance_200';label='Let the controller respond →';
 }else if(fan?.state==='Cancelled'){
  title='The controller confirmed the stop.';text='The fan action is cancelled. The door remains blocked because its predecessor did not complete. Restoring evidence will not restart this cancelled action.';control='reset';label='Restart demonstration ↺';
 }else if(fan?.state==='Executing'){
  title='The fan is running with current support.';text='Remove the pressure reading. The physical pressure stays the same, but the runtime can no longer confirm that it is acceptable.';control='pressure_drop';label='Remove pressure evidence →';
 }else if(state.time_ms===0){
  title='Let the alarm finish and the fan start.';text='Advance 500 milliseconds of simulated time. Watch the runtime and physical device states separately.';control='advance_500';label='Start demonstration →';
 }else{
  title='Explore the current state.';text='Use the controls to advance time or reset to repeat the guided example.';control='reset';label='Restart demonstration ↺';
 }
 $('guide-title').textContent=title;$('guide-text').textContent=text;$('guide').textContent=label;$('guide').onclick=()=>request(control,control==='reset'?state.preset:undefined);
}
document.querySelectorAll('[data-control]').forEach(b=>b.onclick=()=>request(b.dataset.control));
$('preset').onchange=()=>request('reset',$('preset').value);
$('reset').onclick=()=>request('reset',state?.preset||'pressure');
$('planner-mode').onchange=()=>request('reset',state?.preset||'pressure',$('planner-mode').value);
$('generate-plan').onclick=()=>request('generate_plan');
request();
