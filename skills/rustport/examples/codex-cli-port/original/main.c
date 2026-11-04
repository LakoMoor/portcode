#include <stdio.h>
#include <stdint.h>
#include <string.h>
int main(void) {
 uint32_t rng=12345; int x=0,v=0,t=0; char cmd[32],path[256];
 while(scanf("%31s",cmd)==1) {
  if(!strcmp(cmd,"left"))v=-2;
  else if(!strcmp(cmd,"right"))v=2;
  else if(!strcmp(cmd,"stop"))v=0;
  else if(!strcmp(cmd,"tick")){rng=rng*1664525u+1013904223u;x+=v+(int)(rng%3)-1;t++;}
  else if(!strcmp(cmd,"save")){if(scanf("%255s",path)!=1)return 2;FILE*f=fopen(path,"w");if(!f)return 3;fprintf(f,"%d %d %d %u\n",t,x,v,rng);fclose(f);}
  else if(!strcmp(cmd,"load")){if(scanf("%255s",path)!=1)return 2;FILE*f=fopen(path,"r");if(!f)return 3;int n=fscanf(f,"%d %d %d %u",&t,&x,&v,&rng);fclose(f);if(n!=4)return 4;}
  else if(strcmp(cmd,"state"))return 5;
  printf("%d %d %d %u\n",t,x,v,rng);
 }
 return 0;
}
