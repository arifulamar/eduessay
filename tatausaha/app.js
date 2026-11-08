import {API_BASE} from './config.js';
const $=id=>document.getElementById(id),form=$('form');
const today=new Date();$('tanggalPengajuan').value=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Jakarta'}).format(today);
function update(){const a=$('aktif').checked,s=$('skhun').checked;for(const [id,on] of [['kelasWrap',a],['ortuWrap',s],['alamatWrap',s],['tahunWrap',s]]){const e=$(id);e.hidden=!on;e.querySelector('input,textarea').required=on;}}
$('aktif').onchange=update;$('skhun').onchange=update;update();
async function request(path,options={}){const r=await fetch(API_BASE+path,{...options,headers:{'Content-Type':'application/json',...(options.headers||{})}});const data=await r.json().catch(()=>({}));if(!r.ok)throw Error(data.error||'Gagal menghubungi server');return data;}
form.onsubmit=async e=>{e.preventDefault();if(!$('aktif').checked&&!$('skhun').checked){$('message').textContent='Pilih setidaknya satu surat.';return;}const data=Object.fromEntries(new FormData(form));data.jenis=[...($('aktif').checked?['aktif']:[]),...($('skhun').checked?['skhun']:[])];$('submit').disabled=true;$('message').textContent='Mengirim...';try{const res=await request('/api/pengajuan',{method:'POST',body:JSON.stringify(data)});$('message').textContent='Pengajuan diterima. Simpan kode pelacakan ini: '+res.kode+'\nKode diperlukan untuk cek status.';$('tracking').value=res.kode;form.reset();$('tanggalPengajuan').value=data.tanggalPengajuan;update();}catch(err){$('message').textContent=err.message;}finally{$('submit').disabled=false;}};
async function downloadPublicPdf(kode,jenis,button){
  button.disabled=true;
  try{
    const response=await fetch(API_BASE+'/api/status/'+encodeURIComponent(kode)+'/surat/'+encodeURIComponent(jenis)+'/pdf');
    if(!response.ok){
      const data=await response.json().catch(()=>({}));
      throw Error(data.error||'Gagal mengunduh surat');
    }
    const blob=await response.blob();
    const url=URL.createObjectURL(blob);
    const a=document.createElement('a');
    a.href=url;
    a.download='surat-'+jenis+'.pdf';
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(()=>URL.revokeObjectURL(url),5000);
  }catch(error){alert(error.message);}
  finally{button.disabled=false;}
}
$('check').onclick=async()=>{
  const kode=$('tracking').value.trim();
  if(!kode)return;
  const result=$('result');
  result.replaceChildren();
  result.textContent='Memeriksa...';
  try{
    const r=await request('/api/status/'+encodeURIComponent(kode));
    result.replaceChildren();
    if(!r.surat?.length){result.textContent='Status surat belum tersedia.';return;}
    for(const item of r.surat){
      const line=document.createElement('div');
      const label=item.jenis==='aktif'?'Surat Aktif':'Surat SKHUN';
      line.append(document.createTextNode(label+': '+item.status+(item.catatan?' — '+item.catatan:'')));
      if(item.status==='diterbitkan'){
        const button=document.createElement('button');
        button.type='button';
        button.className='download-status';
        button.textContent='Unduh '+label;
        button.onclick=()=>downloadPublicPdf(kode,item.jenis,button);
        line.append(document.createTextNode(' '),button);
      }
      result.append(line);
    }
  }catch(e){result.textContent=e.message;}
};
