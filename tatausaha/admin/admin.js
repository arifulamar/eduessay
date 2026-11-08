import {API_BASE,FIREBASE_CONFIG} from '../config.js';
import {initializeApp} from 'https://www.gstatic.com/firebasejs/11.6.0/firebase-app.js';
import {getAuth,GoogleAuthProvider,signInWithPopup,signOut,onAuthStateChanged} from 'https://www.gstatic.com/firebasejs/11.6.0/firebase-auth.js';
const $=id=>document.getElementById(id),auth=getAuth(initializeApp(FIREBASE_CONFIG));let user=null;
async function api(path,opts={}){const token=await user.getIdToken();const r=await fetch(API_BASE+path,{...opts,headers:{'Authorization':'Bearer '+token,...(opts.body?{'Content-Type':'application/json'}:{})}});if(opts.binary){if(!r.ok)throw Error((await r.json().catch(()=>({}))).error||'Gagal membuat PDF');return r.blob();}const v=await r.json().catch(()=>({}));if(!r.ok)throw Error(v.error||'Permintaan gagal');return v;}
$('loginBtn').onclick=()=>signInWithPopup(auth,new GoogleAuthProvider()).catch(e=>$('notice').textContent=e.message);$('logout').onclick=()=>signOut(auth);$('refresh').onclick=load;$('filter').onchange=load;
$('addStaff').onclick=addStaff;
onAuthStateChanged(auth,async u=>{user=u;$('login').hidden=!!u;$('logout').hidden=!u;$('dashboard').hidden=!u;$('staffPanel').hidden=true;$('who').textContent=u?.email||'';$('items').replaceChildren();$('notice').textContent='';if(u){try{const me=await api('/api/admin/me');$('who').textContent=me.email+' · '+(me.role==='superadmin'?'SUPERADMIN':'STAF TU');if(me.role==='superadmin'){ $('staffPanel').hidden=false;await loadStaff();}await load();}catch(e){$('notice').textContent='Akses belum dapat diverifikasi: '+e.message+' Silakan masuk kembali atau hubungi Superadmin.';$('login').hidden=false;$('dashboard').hidden=true;$('staffPanel').hidden=true;$('logout').hidden=false;$('loginBtn').textContent='Masuk kembali dengan Google';}}});
const el=(tag,text)=>{const n=document.createElement(tag);n.textContent=text;return n;};

let previewUrl=null,previewFilename='surat.pdf';
async function previewPdf(id,jenis,nisn){
  try{
    const blob=await api(`/api/admin/pengajuan/${id}/surat/${jenis}/pdf`,{binary:true});
    if(previewUrl)URL.revokeObjectURL(previewUrl);
    previewUrl=URL.createObjectURL(new Blob([blob],{type:'application/pdf'}));
    previewFilename=`surat-${jenis}-${nisn}.pdf`;
    $('pdfFrame').src=previewUrl;
    $('pdfPreview').hidden=false;
    document.body.style.overflow='hidden';
  }catch(e){alert(e.message);}
}
function closePdfPreview(){
  $('pdfPreview').hidden=true;
  $('pdfFrame').src='about:blank';
  if(previewUrl){URL.revokeObjectURL(previewUrl);previewUrl=null;}
  document.body.style.overflow='';
}
$('closePdf').onclick=closePdfPreview;
$('downloadPdf').onclick=()=>{
  if(!previewUrl)return;
  const a=document.createElement('a');a.href=previewUrl;a.download=previewFilename;a.click();
};
$('pdfPreview').addEventListener('click',e=>{if(e.target===$('pdfPreview'))closePdfPreview();});

async function load(){try{const data=await api('/api/admin/pengajuan');const list=$('items');list.replaceChildren();const filter=$('filter').value;let count=0;for(const p of data){for(const s of p.surat){if(filter&&s.status!==filter)continue;count++;const box=el('article','');box.className='item';box.append(el('h3',p.nama),el('p',`NISN: ${p.nisn} · ${s.jenis==='aktif'?'Surat Aktif':'Surat SKHUN'}`),el('p',`Kode pengajuan: ${p.kodePengajuan||'Hanya tersedia untuk pengajuan baru'}`),el('p',`TTL: ${p.tempatLahir}, ${p.tanggalLahir} · Pengajuan: ${p.tanggalPengajuan}`),el('p',`Kelas: ${p.kelas||'-'} · Orang tua: ${p.namaOrtu||'-'} · Tahun lulus: ${p.tahunLulus||'-'}`),el('p',`Alamat: ${p.alamat||'-'}`));const badge=el('span',s.status);badge.className='pill';box.append(badge);const controls=el('div','');controls.className='controls';const number=el('input','');number.placeholder='Nomor surat lengkap';number.value=s.nomor||'';const note=el('textarea','');note.placeholder='Catatan verifikasi (opsional)';note.value=s.catatan||'';controls.append(number,note);box.append(controls);const actions=el('div','');actions.className='actions';for(const [status,label] of [['perbaikan','Minta perbaikan'],['ditolak','Tolak'],['diterbitkan','Terbitkan surat']]){const btn=el('button',label);if(status!=='diterbitkan')btn.className='secondary';btn.onclick=async()=>{if(status==='diterbitkan'&&!number.value.trim()){alert('Isi nomor surat.');return;}if(status==='diterbitkan'&&!confirm('Terbitkan surat dengan nomor ini?'))return;btn.disabled=true;try{await api(`/api/admin/pengajuan/${p.id}/surat/${s.jenis}`,{method:'PATCH',body:JSON.stringify({status,nomor:number.value.trim(),catatan:note.value.trim()})});await load();}catch(e){alert(e.message);}finally{btn.disabled=false;}};actions.append(btn);}if(s.status==='diterbitkan'){const pdf=el('button','Pratinjau PDF');pdf.className='secondary';pdf.onclick=()=>previewPdf(p.id,s.jenis,p.nisn);actions.append(pdf);}box.append(actions);list.append(box);}}if(!count)list.append(el('p','Tidak ada pengajuan untuk filter ini.'));}catch(e){$('notice').textContent=e.message;}}
async function loadStaff(){try{const rows=await api('/api/superadmin/staff');const list=$('staffList');list.replaceChildren();if(!rows.length){list.append(el('p','Belum ada staf tambahan.'));return;}for(const row of rows){const item=el('div','');item.className='item';const label=el('b',row.email);const del=el('button','Hapus akses');del.className='secondary';del.onclick=async()=>{if(!confirm('Cabut akses staf '+row.email+'?'))return;try{await api('/api/superadmin/staff/'+encodeURIComponent(row.email),{method:'DELETE'});await loadStaff();}catch(e){alert(e.message);}};item.append(label,del);list.append(item);}}catch(e){$('staffList').textContent=e.message;}}
async function addStaff(){const email=$('staffEmail').value.trim();if(!email)return;const btn=$('addStaff');btn.disabled=true;try{await api('/api/superadmin/staff',{method:'POST',body:JSON.stringify({email})});$('staffEmail').value='';await loadStaff();alert('Akses staf berhasil ditambahkan.');}catch(e){alert(e.message);}finally{btn.disabled=false;}}
