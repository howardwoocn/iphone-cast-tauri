#include "geometry.h"
#include <cstdlib>
#include <iostream>
static void check(bool condition,const char*message){if(!condition){std::cerr<<message<<'\n';std::exit(1);}}
int main(){
    auto portrait=cast::fit(1000,700,1179,2556);
    check(portrait.height==700&&portrait.width==323&&portrait.x==338,"portrait fit");
    auto landscape=cast::fit(400,800,2556,1179);
    check(landscape.width==400&&landscape.height==185&&landscape.y==307,"landscape fit");
    auto invalid=cast::fit(0,100,1179,2556);check(invalid.width==0,"zero area");
    double x,y;check(cast::normalize(0,0,323,700,x,y)&&x==0&&y==0,"top-left");
    check(cast::normalize(322,699,323,700,x,y)&&x==1&&y==1,"bottom-right");
    check(!cast::normalize(323,0,323,700,x,y),"outside");
    auto dpi=cast::fit(1500,1050,1179,2556);
    check(dpi.height==1050&&dpi.width==484,"150 percent DPI");
    std::cout<<"geometry: 7 checks passed\n";
}
